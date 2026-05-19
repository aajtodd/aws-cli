//! Test backend abstraction for compat testing.
//!
//! Supports both mock S3 (local server with direct API) and prod S3
//! (real AWS endpoint via SDK client).

use aws_sdk_s3::Client;
use s3_mock_server::{ObjectData, ObjectListEntry, S3MockServer, ServerHandle};
use std::collections::HashMap;

use crate::error::{Error, ErrorKind, from_kind};

/// Test backend for running compat specs.
///
/// Provides an SDK client for S3 operations (works against both mock and prod)
/// and optional direct mock server access for mock-only features.
pub struct TestBackend {
    /// SDK client for seeding (prod) and inspection (both).
    client: Client,
    /// Endpoint URL for the CLI subprocess (set as `AWS_ENDPOINT_URL`).
    endpoint_url: Option<String>,
    /// Direct mock server access. `None` for prod backend.
    mock: Option<MockControl>,
    /// Buckets created during this test (for prod cleanup).
    created_buckets: std::sync::Mutex<Vec<String>>,
}

/// Direct access to the mock server for features not available through
/// the S3 protocol: fault injection, request log, timestamp control,
/// and optimized seeding.
pub struct MockControl {
    server: S3MockServer,
    handle: ServerHandle,
}

impl TestBackend {
    /// Create a mock backend: starts a local S3 server.
    pub async fn mock() -> Result<Self, Error> {
        let server = S3MockServer::builder().with_in_memory_store().build()?;
        let handle = server.start().await?;
        let endpoint_url = format!("http://127.0.0.1:{}", handle.socket_addr().port());
        let client = handle.client().await;
        Ok(Self {
            client,
            endpoint_url: Some(endpoint_url),
            mock: Some(MockControl { server, handle }),
            created_buckets: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Create a prod backend: SDK client pointed at real S3.
    pub async fn prod(region: &str) -> Result<Self, Error> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .load()
            .await;
        let client = Client::new(&config);
        Ok(Self {
            client,
            endpoint_url: None,
            mock: None,
            created_buckets: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Endpoint URL for the CLI subprocess. `None` means use default AWS endpoint.
    pub fn endpoint_url(&self) -> Option<&str> {
        self.endpoint_url.as_deref()
    }

    /// SDK client for S3 operations.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Direct mock server access. Returns `None` for prod backend.
    pub fn mock_control(&self) -> Option<&MockControl> {
        self.mock.as_ref()
    }

    /// Create a bucket. Uses direct API on mock, SDK on prod.
    pub async fn create_bucket(&self, bucket: &str) -> Result<(), Error> {
        if let Some(mock) = &self.mock {
            mock.server.create_bucket(bucket).await?;
        } else {
            self.client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await
                .map_err(from_kind(ErrorKind::Sdk))?;
        }
        self.created_buckets
            .lock()
            .unwrap()
            .push(bucket.to_string());
        Ok(())
    }

    /// Seed an object. Uses direct API on mock (supports all fields
    /// including `last_modified`), SDK client on prod.
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
        metadata: Option<HashMap<String, String>>,
        last_modified: Option<std::time::SystemTime>,
    ) -> Result<(), Error> {
        if let Some(mock) = &self.mock {
            let mut req = s3_mock_server::AddObjectRequest::new(body);
            req.metadata = metadata;
            if let Some(ct) = content_type {
                req = req.content_type(ct);
            }
            if let Some(lm) = last_modified {
                req = req.last_modified(lm);
            }
            Ok(mock.server.add_object_with(bucket, key, req).await?)
        } else {
            // SDK path — last_modified can't be set via PutObject.
            let mut req = self
                .client
                .put_object()
                .bucket(bucket)
                .key(key)
                .body(body.into());
            if let Some(ct) = content_type {
                req = req.content_type(ct);
            }
            if let Some(meta) = metadata {
                for (k, v) in meta {
                    req = req.metadata(k, v);
                }
            }
            req.send().await.map_err(from_kind(ErrorKind::Sdk))?;
            Ok(())
        }
    }

    /// Fetch object head metadata and body for assertion.
    ///
    /// Returns `None` if the object doesn't exist.
    /// Uses the SDK client for both mock and prod — the mock path goes through
    /// the mock's HTTP server, so the response structure is identical.
    pub async fn fetch_object_for_assertion(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<
        Option<(
            aws_sdk_s3::operation::head_object::HeadObjectOutput,
            Vec<u8>,
        )>,
        Error,
    > {
        match self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => {
                let body = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| Error::new(ErrorKind::Sdk, e))?
                    .into_bytes()
                    .to_vec();
                let head = aws_sdk_s3::operation::head_object::HeadObjectOutput::builder()
                    .set_content_type(output.content_type)
                    .set_content_length(output.content_length)
                    .set_e_tag(output.e_tag)
                    .set_storage_class(output.storage_class)
                    .set_server_side_encryption(output.server_side_encryption)
                    .set_checksum_crc32(output.checksum_crc32)
                    .set_checksum_crc32_c(output.checksum_crc32_c)
                    .set_checksum_crc64_nvme(output.checksum_crc64_nvme)
                    .set_checksum_sha1(output.checksum_sha1)
                    .set_checksum_sha256(output.checksum_sha256)
                    .set_checksum_type(output.checksum_type)
                    .set_metadata(output.metadata)
                    .build();
                Ok(Some((head, body)))
            }
            Err(sdk_err) => {
                if sdk_err
                    .as_service_error()
                    .is_some_and(|e| e.is_no_such_key())
                {
                    Ok(None)
                } else {
                    Err(Error::new(ErrorKind::Sdk, sdk_err))
                }
            }
        }
    }

    /// Get object content and metadata.
    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectData>, Error> {
        if let Some(mock) = &self.mock {
            return Ok(mock.server.get_object(bucket, key).await?);
        }
        match self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => {
                let content_type = output.content_type().map(|s| s.to_string());
                let content_length = output.content_length.unwrap_or(0) as u64;
                let etag = output.e_tag().unwrap_or_default().to_string();
                let last_modified = output
                    .last_modified()
                    .and_then(|t| std::time::SystemTime::try_from(*t).ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let metadata = output.metadata().cloned().unwrap_or_default();
                let body = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| Error::new(ErrorKind::Sdk, e))?
                    .into_bytes();
                Ok(Some(ObjectData {
                    body,
                    content_type,
                    content_length,
                    etag,
                    last_modified,
                    metadata,
                }))
            }
            Err(sdk_err) => {
                if sdk_err
                    .as_service_error()
                    .is_some_and(|e| e.is_no_such_key())
                {
                    Ok(None)
                } else {
                    Err(Error::new(ErrorKind::Sdk, sdk_err))
                }
            }
        }
    }

    /// Check if an object exists.
    pub async fn object_exists(&self, bucket: &str, key: &str) -> Result<bool, Error> {
        if let Some(mock) = &self.mock {
            return Ok(mock.server.object_exists(bucket, key).await?);
        }
        match self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(sdk_err) => {
                if sdk_err.as_service_error().is_some_and(|e| e.is_not_found()) {
                    Ok(false)
                } else {
                    Err(Error::new(ErrorKind::Sdk, sdk_err))
                }
            }
        }
    }

    /// List objects in a bucket with optional prefix filter.
    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<ObjectListEntry>, Error> {
        if let Some(mock) = &self.mock {
            return Ok(mock.server.list_objects(bucket, prefix).await?);
        }
        let mut req = self.client.list_objects_v2().bucket(bucket);
        if let Some(p) = prefix {
            req = req.prefix(p);
        }
        let resp = req.send().await.map_err(from_kind(ErrorKind::Sdk))?;
        let entries = resp
            .contents()
            .iter()
            .map(|obj| ObjectListEntry {
                key: obj.key().unwrap_or_default().to_string(),
                size: obj.size().unwrap_or(0) as u64,
                last_modified: obj
                    .last_modified()
                    .and_then(|t| std::time::SystemTime::try_from(*t).ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                etag: obj.e_tag().unwrap_or_default().to_string(),
            })
            .collect();
        Ok(entries)
    }

    /// Reset all state. Mock: clears everything. Prod: empties and deletes tracked buckets.
    pub async fn reset(&self) -> Result<(), Error> {
        if let Some(mock) = &self.mock {
            self.created_buckets.lock().unwrap().clear();
            return Ok(mock.server.reset().await?);
        }
        let buckets: Vec<String> = self.created_buckets.lock().unwrap().drain(..).collect();
        for bucket in &buckets {
            self.empty_bucket(bucket).await?;
            self.client
                .delete_bucket()
                .bucket(bucket)
                .send()
                .await
                .map_err(from_kind(ErrorKind::Sdk))?;
        }
        Ok(())
    }

    /// Delete all objects in a bucket via list + batch delete.
    /// Empty a bucket and delete it. Used for stale cleanup.
    pub async fn empty_and_delete_bucket(&self, bucket: &str) -> Result<(), Error> {
        self.empty_bucket(bucket).await?;
        self.client
            .delete_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(from_kind(ErrorKind::Sdk))?;
        Ok(())
    }

    async fn empty_bucket(&self, bucket: &str) -> Result<(), Error> {
        loop {
            let resp = self
                .client
                .list_objects_v2()
                .bucket(bucket)
                .send()
                .await
                .map_err(from_kind(ErrorKind::Sdk))?;
            let objects: Vec<_> = resp
                .contents()
                .iter()
                .filter_map(|o| {
                    o.key().map(|k| {
                        aws_sdk_s3::types::ObjectIdentifier::builder()
                            .key(k)
                            .build()
                            .ok()
                    })
                })
                .flatten()
                .collect();
            if objects.is_empty() {
                break;
            }
            let delete = aws_sdk_s3::types::Delete::builder()
                .set_objects(Some(objects))
                .build()
                .map_err(|e| Error::new(ErrorKind::Sdk, e.to_string()))?;
            self.client
                .delete_objects()
                .bucket(bucket)
                .delete(delete)
                .send()
                .await
                .map_err(from_kind(ErrorKind::Sdk))?;
        }
        Ok(())
    }

    /// Shut down the backend. For mock, stops the server.
    pub async fn shutdown(self) -> Result<(), Error> {
        // Clean up any buckets created during this backend's lifetime
        self.reset().await?;
        if let Some(mock) = self.mock {
            Ok(mock.handle.shutdown().await?)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUCKET: &str = "test-bucket";

    #[tokio::test]
    async fn test_create_bucket_and_put_object() {
        let backend = TestBackend::mock().await.unwrap();
        backend.create_bucket(BUCKET).await.unwrap();
        backend
            .put_object(BUCKET, "hello.txt", b"hello".to_vec(), None, None, None)
            .await
            .unwrap();

        let obj = backend
            .get_object(BUCKET, "hello.txt")
            .await
            .unwrap()
            .expect("object should exist");
        assert_eq!(obj.body.as_ref(), b"hello");
        backend.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_object_exists() {
        let backend = TestBackend::mock().await.unwrap();
        backend.create_bucket(BUCKET).await.unwrap();
        backend
            .put_object(BUCKET, "exists.txt", b"data".to_vec(), None, None, None)
            .await
            .unwrap();

        assert!(backend.object_exists(BUCKET, "exists.txt").await.unwrap());
        assert!(!backend.object_exists(BUCKET, "nope.txt").await.unwrap());
        backend.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_put_with_metadata() {
        let backend = TestBackend::mock().await.unwrap();
        backend.create_bucket(BUCKET).await.unwrap();

        let meta = HashMap::from([("author".to_string(), "test-user".to_string())]);
        backend
            .put_object(
                BUCKET,
                "doc.txt",
                b"content".to_vec(),
                Some("text/plain"),
                Some(meta),
                None,
            )
            .await
            .unwrap();

        let obj = backend
            .get_object(BUCKET, "doc.txt")
            .await
            .unwrap()
            .expect("object should exist");
        assert_eq!(obj.body.as_ref(), b"content");
        assert_eq!(
            obj.metadata.get("author").map(String::as_str),
            Some("test-user")
        );
        backend.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_objects() {
        let backend = TestBackend::mock().await.unwrap();
        backend.create_bucket(BUCKET).await.unwrap();
        backend
            .put_object(BUCKET, "a/one.txt", b"1".to_vec(), None, None, None)
            .await
            .unwrap();
        backend
            .put_object(BUCKET, "a/two.txt", b"2".to_vec(), None, None, None)
            .await
            .unwrap();
        backend
            .put_object(BUCKET, "b/three.txt", b"3".to_vec(), None, None, None)
            .await
            .unwrap();

        let listed = backend.list_objects(BUCKET, Some("a/")).await.unwrap();
        let keys: Vec<&str> = listed.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"a/one.txt"));
        assert!(keys.contains(&"a/two.txt"));
        backend.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_reset() {
        let backend = TestBackend::mock().await.unwrap();
        backend.create_bucket(BUCKET).await.unwrap();
        backend
            .put_object(BUCKET, "file.txt", b"data".to_vec(), None, None, None)
            .await
            .unwrap();

        backend.reset().await.unwrap();

        // After reset, bucket is gone — recreate to list.
        backend.create_bucket(BUCKET).await.unwrap();
        let listed = backend.list_objects(BUCKET, None).await.unwrap();
        assert!(listed.is_empty());
        backend.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_mock_backend_reuse_after_reset() {
        let backend = TestBackend::mock().await.unwrap();
        let endpoint = backend.endpoint_url().unwrap().to_string();

        // First use
        backend.create_bucket("reuse-bucket-1").await.unwrap();
        backend
            .put_object(
                "reuse-bucket-1",
                "key1",
                b"data1".to_vec(),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let obj = backend.get_object("reuse-bucket-1", "key1").await.unwrap();
        assert!(obj.is_some());

        // Reset
        backend.reset().await.unwrap();

        // Second use on same backend
        backend.create_bucket("reuse-bucket-2").await.unwrap();
        backend
            .put_object(
                "reuse-bucket-2",
                "key2",
                b"data2".to_vec(),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let obj = backend.get_object("reuse-bucket-2", "key2").await.unwrap();
        assert!(obj.is_some());
        assert_eq!(obj.unwrap().body.as_ref(), b"data2");

        // Old bucket gone
        let old = backend.get_object("reuse-bucket-1", "key1").await.unwrap();
        assert!(old.is_none());

        // Verify server still accepts raw TCP connections
        let addr = endpoint.strip_prefix("http://").unwrap();
        let stream = tokio::net::TcpStream::connect(addr).await;
        assert!(
            stream.is_ok(),
            "Server should accept TCP connections after reset"
        );

        backend.shutdown().await.unwrap();
    }
}
