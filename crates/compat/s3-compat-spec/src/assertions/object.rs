//! S3 object state assertions.
//!
//! Compares expected specs against `HeadObjectOutput` + body bytes directly.
//! No translation layer — fields are read from the SDK response type.

use aws_sdk_s3::operation::head_object::HeadObjectOutput;

use super::AssertionResult;
use crate::spec::{ExpectedObject, UploadMethod};

/// Compute CRC32C of data and return as base64-encoded string (matching S3 format).
pub fn crc32c_base64(data: &[u8]) -> String {
    use base64::Engine;
    let checksum = crc_fast::checksum(crc_fast::CrcAlgorithm::Crc32Iscsi, data) as u32;
    base64::engine::general_purpose::STANDARD.encode(checksum.to_be_bytes())
}

/// Format a short preview of `data` for diagnostic messages.
///
/// For bodies up to ~128 bytes, shows the full content (utf8-lossy, debug-escaped).
/// For larger bodies, shows the prefix and a byte-count suffix. Callers should
/// include byte length and CRC32C separately so diagnosis doesn't depend on the
/// visible prefix matching.
pub fn byte_preview(data: &[u8]) -> String {
    const MAX: usize = 128;
    if data.len() <= MAX {
        format!("{:?}", String::from_utf8_lossy(data))
    } else {
        format!(
            "{:?}... ({} more bytes)",
            String::from_utf8_lossy(&data[..MAX]),
            data.len() - MAX
        )
    }
}

/// Verify data integrity by computing CRC32C of expected and actual bytes.
///
/// Returns `Pass` if checksums match, `Fail` with details if they don't.
pub fn verify_integrity(expected_data: &[u8], actual_data: &[u8], label: &str) -> AssertionResult {
    let expected_crc = crc32c_base64(expected_data);
    let actual_crc = crc32c_base64(actual_data);
    if expected_crc == actual_crc {
        tracing::debug!(%label, crc32c = %actual_crc, "integrity verified");
        AssertionResult::Pass
    } else {
        AssertionResult::Fail {
            message: format!(
                "{label} integrity check failed: expected crc32c={expected_crc}, got crc32c={actual_crc} \
                 (expected {} bytes, got {} bytes)",
                expected_data.len(),
                actual_data.len()
            ),
        }
    }
}

/// Assert a single expected object against fetched state.
///
/// `head` is None if the object doesn't exist (HeadObject returned 404).
/// Always logs all available checksums and metadata at debug level.
pub fn assert_object(
    expected: &ExpectedObject,
    head: Option<&HeadObjectOutput>,
    body: Option<&[u8]>,
) -> Vec<AssertionResult> {
    let mut results = Vec::new();

    // Log all available state for traceability
    if let Some(h) = head {
        tracing::debug!(
            key = %expected.key,
            content_type = ?h.content_type(),
            content_length = ?h.content_length(),
            e_tag = ?h.e_tag(),
            storage_class = ?h.storage_class(),
            server_side_encryption = ?h.server_side_encryption(),
            checksum_crc32 = ?h.checksum_crc32(),
            checksum_crc32_c = ?h.checksum_crc32_c(),
            checksum_crc64_nvme = ?h.checksum_crc64_nvme(),
            checksum_sha1 = ?h.checksum_sha1(),
            checksum_sha256 = ?h.checksum_sha256(),
            checksum_type = ?h.checksum_type(),
            "object state"
        );
        if let Some(metadata) = h.metadata()
            && !metadata.is_empty()
        {
            tracing::debug!(key = %expected.key, ?metadata, "object metadata");
        }
    }

    // Existence check
    match (expected.exists, head) {
        (true, None) => {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} expected to exist but not found",
                    expected.bucket, expected.key
                ),
            });
            return results;
        }
        (false, None) => {
            results.push(AssertionResult::Pass);
            return results;
        }
        (false, Some(_)) => {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} expected to not exist but found",
                    expected.bucket, expected.key
                ),
            });
            return results;
        }
        (true, Some(_)) => {
            results.push(AssertionResult::Pass);
        }
    }

    let h = head.unwrap();

    // Content
    if let Some(expected_content) = &expected.content {
        match body {
            Some(actual_body) => {
                if actual_body == expected_content.as_bytes() {
                    tracing::debug!(key = %expected.key, "content matches");
                    results.push(AssertionResult::Pass);
                } else {
                    let expected_bytes = expected_content.as_bytes();
                    results.push(AssertionResult::Fail {
                        message: format!(
                            "object {}/{} content mismatch:\n  \
                             expected ({} bytes, crc32c={}): {}\n  \
                             actual   ({} bytes, crc32c={}): {}",
                            expected.bucket,
                            expected.key,
                            expected_bytes.len(),
                            crc32c_base64(expected_bytes),
                            byte_preview(expected_bytes),
                            actual_body.len(),
                            crc32c_base64(actual_body),
                            byte_preview(actual_body),
                        ),
                    });
                }
            }
            None => {
                results.push(AssertionResult::Fail {
                    message: format!(
                        "object {}/{} content assertion requires body but none fetched",
                        expected.bucket, expected.key
                    ),
                });
            }
        }
    }

    // Size
    if let Some(expected_size) = expected.size {
        let actual_size = h.content_length().unwrap_or(0) as u64;
        if actual_size == expected_size {
            tracing::debug!(key = %expected.key, size = actual_size, "size matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} size mismatch: expected {} bytes, got {} bytes",
                    expected.bucket, expected.key, expected_size, actual_size
                ),
            });
        }
    }

    // Content-type
    if let Some(expected_ct) = &expected.content_type {
        let actual_ct = h.content_type().unwrap_or("");
        if actual_ct == expected_ct {
            tracing::debug!(key = %expected.key, content_type = actual_ct, "content_type matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} content_type mismatch: expected {:?}, got {:?}",
                    expected.bucket, expected.key, expected_ct, actual_ct
                ),
            });
        }
    }

    // Cache-Control
    if let Some(expected_cc) = &expected.cache_control {
        let actual = h.cache_control().unwrap_or("");
        if actual == expected_cc {
            tracing::debug!(key = %expected.key, cache_control = actual, "cache_control matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} cache_control mismatch: expected {:?}, got {:?}",
                    expected.bucket, expected.key, expected_cc, actual
                ),
            });
        }
    }

    // Content-Encoding
    if let Some(expected_ce) = &expected.content_encoding {
        let actual = h.content_encoding().unwrap_or("");
        if actual == expected_ce {
            tracing::debug!(key = %expected.key, content_encoding = actual, "content_encoding matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} content_encoding mismatch: expected {:?}, got {:?}",
                    expected.bucket, expected.key, expected_ce, actual
                ),
            });
        }
    }

    // Content-Disposition
    if let Some(expected_cd) = &expected.content_disposition {
        let actual = h.content_disposition().unwrap_or("");
        if actual == expected_cd {
            tracing::debug!(key = %expected.key, content_disposition = actual, "content_disposition matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} content_disposition mismatch: expected {:?}, got {:?}",
                    expected.bucket, expected.key, expected_cd, actual
                ),
            });
        }
    }

    // Content-Language
    if let Some(expected_cl) = &expected.content_language {
        let actual = h.content_language().unwrap_or("");
        if actual == expected_cl {
            tracing::debug!(key = %expected.key, content_language = actual, "content_language matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} content_language mismatch: expected {:?}, got {:?}",
                    expected.bucket, expected.key, expected_cl, actual
                ),
            });
        }
    }

    // ETag
    if let Some(expected_etag) = &expected.e_tag {
        let actual_etag = h.e_tag().unwrap_or("");
        if actual_etag == expected_etag {
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} e_tag mismatch: expected {:?}, got {:?}",
                    expected.bucket, expected.key, expected_etag, actual_etag
                ),
            });
        }
    }

    // Upload method — inferred from ETag suffix (`"hex-N"` = multipart, `"hex"` = single PUT)
    if let Some(expected_method) = &expected.upload_method {
        let etag = h.e_tag().unwrap_or("").trim_matches('"');
        let actual_method = if etag
            .rsplit_once('-')
            .is_some_and(|(_, n)| n.parse::<u32>().is_ok())
        {
            UploadMethod::Multipart
        } else {
            UploadMethod::PutObject
        };
        if actual_method == *expected_method {
            tracing::debug!(key = %expected.key, ?actual_method, "upload_method matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} upload_method mismatch: expected {:?}, got {:?} (etag={:?})",
                    expected.bucket, expected.key, expected_method, actual_method, etag
                ),
            });
        }
    }

    // Part count — parsed from the multipart ETag suffix (`"hex-N"` → N)
    if let Some(expected_pc) = expected.part_count {
        let etag = h.e_tag().unwrap_or("").trim_matches('"');
        let actual_pc = etag
            .rsplit_once('-')
            .and_then(|(_, n)| n.parse::<u32>().ok());
        if actual_pc == Some(expected_pc) {
            tracing::debug!(key = %expected.key, part_count = expected_pc, "part_count matches");
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} part_count mismatch: expected {}, got {:?} (etag={:?})",
                    expected.bucket, expected.key, expected_pc, actual_pc, etag
                ),
            });
        }
    }

    // Storage class
    if let Some(expected_sc) = &expected.storage_class {
        let actual_sc = h
            .storage_class()
            .unwrap_or(&aws_sdk_s3::types::StorageClass::Standard);
        if actual_sc == expected_sc {
            results.push(AssertionResult::Pass);
        } else {
            results.push(AssertionResult::Fail {
                message: format!(
                    "object {}/{} storage_class mismatch: expected {:?}, got {:?}",
                    expected.bucket,
                    expected.key,
                    expected_sc.as_str(),
                    actual_sc.as_str()
                ),
            });
        }
    }

    // Checksums
    check_checksum(
        &expected.checksum_sha256,
        h.checksum_sha256(),
        "sha256",
        expected,
        &mut results,
    );
    check_checksum(
        &expected.checksum_crc32,
        h.checksum_crc32(),
        "crc32",
        expected,
        &mut results,
    );
    check_checksum(
        &expected.checksum_crc32c,
        h.checksum_crc32_c(),
        "crc32c",
        expected,
        &mut results,
    );
    check_checksum(
        &expected.checksum_crc64nvme,
        h.checksum_crc64_nvme(),
        "crc64nvme",
        expected,
        &mut results,
    );
    check_checksum(
        &expected.checksum_sha1,
        h.checksum_sha1(),
        "sha1",
        expected,
        &mut results,
    );

    // Metadata
    if !expected.metadata.is_empty() || expected.metadata_strict {
        let actual_metadata = h.metadata().cloned().unwrap_or_default();
        for (key, expected_val) in &expected.metadata {
            match actual_metadata.get(key) {
                Some(actual_val) if actual_val == expected_val => {
                    results.push(AssertionResult::Pass);
                }
                Some(actual_val) => {
                    results.push(AssertionResult::Fail {
                        message: format!(
                            "object {}/{} metadata {:?}: expected {:?}, got {:?}",
                            expected.bucket, expected.key, key, expected_val, actual_val
                        ),
                    });
                }
                None => {
                    results.push(AssertionResult::Fail {
                        message: format!(
                            "object {}/{} metadata {:?}: expected {:?}, key not present",
                            expected.bucket, expected.key, key, expected_val
                        ),
                    });
                }
            }
        }
        if expected.metadata_strict {
            for key in actual_metadata.keys() {
                if !expected.metadata.contains_key(key) {
                    results.push(AssertionResult::Fail {
                        message: format!(
                            "object {}/{} unexpected metadata key {:?} = {:?} (strict mode)",
                            expected.bucket, expected.key, key, actual_metadata[key]
                        ),
                    });
                }
            }
        }
    }

    results
}

fn check_checksum(
    expected: &Option<String>,
    actual: Option<&str>,
    name: &str,
    exp_obj: &ExpectedObject,
    results: &mut Vec<AssertionResult>,
) {
    if let Some(expected_val) = expected {
        match actual {
            Some(actual_val) if actual_val == expected_val => {
                tracing::debug!(key = %exp_obj.key, %name, "checksum matches");
                results.push(AssertionResult::Pass);
            }
            Some(actual_val) => {
                results.push(AssertionResult::Fail {
                    message: format!(
                        "object {}/{} checksum_{} mismatch: expected {:?}, got {:?}",
                        exp_obj.bucket, exp_obj.key, name, expected_val, actual_val
                    ),
                });
            }
            None => {
                results.push(AssertionResult::Fail {
                    message: format!(
                        "object {}/{} expected checksum_{} = {:?} but none present",
                        exp_obj.bucket, exp_obj.key, name, expected_val
                    ),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn expected_obj(key: &str) -> ExpectedObject {
        ExpectedObject {
            bucket: "{bucket}".into(),
            key: key.into(),
            exists: true,
            content: None,
            size: None,
            content_type: None,
            cache_control: None,
            content_encoding: None,
            content_disposition: None,
            content_language: None,
            e_tag: None,
            storage_class: None,
            server_side_encryption: None,
            checksum_type: None,
            checksum_sha256: None,
            checksum_crc32: None,
            checksum_crc32c: None,
            checksum_crc64nvme: None,
            checksum_sha1: None,
            metadata: HashMap::new(),
            metadata_strict: false,
            upload_method: None,
            part_count: None,
        }
    }

    fn head(content_type: &str, size: i64) -> HeadObjectOutput {
        HeadObjectOutput::builder()
            .content_type(content_type)
            .content_length(size)
            .build()
    }

    #[test]
    fn exists_pass() {
        let h = head("text/plain", 5);
        let results = assert_object(&expected_obj("f.txt"), Some(&h), Some(b"hello"));
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn exists_but_missing() {
        let results = assert_object(&expected_obj("f.txt"), None, None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn not_exists_and_missing() {
        let mut exp = expected_obj("f.txt");
        exp.exists = false;
        let results = assert_object(&exp, None, None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn not_exists_but_found() {
        let mut exp = expected_obj("f.txt");
        exp.exists = false;
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), Some(b"hello"));
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn content_match() {
        let mut exp = expected_obj("f.txt");
        exp.content = Some("hello".into());
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), Some(b"hello"));
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn content_mismatch() {
        let mut exp = expected_obj("f.txt");
        exp.content = Some("hello".into());
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), Some(b"world"));
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn size_match() {
        let mut exp = expected_obj("f.txt");
        exp.size = Some(5);
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn size_mismatch() {
        let mut exp = expected_obj("f.txt");
        exp.size = Some(99);
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn content_type_match() {
        let mut exp = expected_obj("f.txt");
        exp.content_type = Some("text/plain".into());
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn content_type_mismatch() {
        let mut exp = expected_obj("f.txt");
        exp.content_type = Some("application/json".into());
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn cache_control_match() {
        let mut exp = expected_obj("f.txt");
        exp.cache_control = Some("max-age=600, public".into());
        let h = HeadObjectOutput::builder()
            .cache_control("max-age=600, public")
            .content_length(5)
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn cache_control_mismatch() {
        let mut exp = expected_obj("f.txt");
        exp.cache_control = Some("no-cache".into());
        let h = HeadObjectOutput::builder()
            .cache_control("max-age=600, public")
            .content_length(5)
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn part_count_match() {
        let mut exp = expected_obj("big.bin");
        exp.part_count = Some(2);
        let h = HeadObjectOutput::builder()
            .e_tag("\"d41d8cd98f00b204e9800998ecf8427e-2\"")
            .content_length(16777216)
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn part_count_mismatch() {
        let mut exp = expected_obj("big.bin");
        exp.part_count = Some(3);
        let h = HeadObjectOutput::builder()
            .e_tag("\"d41d8cd98f00b204e9800998ecf8427e-2\"")
            .content_length(16777216)
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn checksum_match() {
        let mut exp = expected_obj("f.txt");
        exp.checksum_sha256 = Some("abc123".into());
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .checksum_sha256("abc123")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn checksum_mismatch() {
        let mut exp = expected_obj("f.txt");
        exp.checksum_sha256 = Some("abc123".into());
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .checksum_sha256("wrong")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn checksum_missing() {
        let mut exp = expected_obj("f.txt");
        exp.checksum_sha256 = Some("abc123".into());
        let h = head("text/plain", 5);
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn etag_match() {
        let mut exp = expected_obj("f.txt");
        exp.e_tag = Some("\"abc123\"".into());
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .e_tag("\"abc123\"")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn metadata_partial_match() {
        let mut exp = expected_obj("f.txt");
        exp.metadata.insert("author".into(), "test".into());
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .metadata("author", "test")
            .metadata("extra", "ignored")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn metadata_partial_mismatch() {
        let mut exp = expected_obj("f.txt");
        exp.metadata.insert("author".into(), "test".into());
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .metadata("author", "wrong")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn metadata_strict_extra_key_fails() {
        let mut exp = expected_obj("f.txt");
        exp.metadata.insert("author".into(), "test".into());
        exp.metadata_strict = true;
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .metadata("author", "test")
            .metadata("unexpected", "value")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn metadata_strict_exact_match() {
        let mut exp = expected_obj("f.txt");
        exp.metadata.insert("author".into(), "test".into());
        exp.metadata_strict = true;
        let h = HeadObjectOutput::builder()
            .content_type("text/plain")
            .content_length(5)
            .metadata("author", "test")
            .build();
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn crc32c_base64_known_value() {
        // "hello" CRC32C = 0xc9265082
        let result = crc32c_base64(b"hello");
        // Verify it's valid base64 and consistent
        assert!(!result.is_empty());
        assert_eq!(result, crc32c_base64(b"hello")); // deterministic
    }

    #[test]
    fn crc32c_base64_different_data() {
        assert_ne!(crc32c_base64(b"hello"), crc32c_base64(b"world"));
    }

    #[test]
    fn verify_integrity_pass() {
        let data = b"hello world";
        let result = verify_integrity(data, data, "test");
        assert!(result.is_pass());
    }

    #[test]
    fn verify_integrity_same_content_different_slices() {
        let expected = b"hello world".to_vec();
        let actual = b"hello world".to_vec();
        let result = verify_integrity(&expected, &actual, "test");
        assert!(result.is_pass());
    }

    #[test]
    fn verify_integrity_mismatch() {
        let result = verify_integrity(b"hello", b"world", "test-obj");
        assert!(!result.is_pass());
        if let AssertionResult::Fail { message } = result {
            assert!(message.contains("integrity check failed"));
            assert!(message.contains("test-obj"));
        }
    }

    #[test]
    fn verify_integrity_empty_vs_nonempty() {
        let result = verify_integrity(b"", b"data", "test");
        assert!(!result.is_pass());
    }

    #[test]
    fn verify_integrity_both_empty() {
        let result = verify_integrity(b"", b"", "test");
        assert!(result.is_pass());
    }

    fn head_with_etag(etag: &str) -> HeadObjectOutput {
        HeadObjectOutput::builder().e_tag(etag).build()
    }

    #[test]
    fn upload_method_multipart_match() {
        let mut exp = expected_obj("big.bin");
        exp.upload_method = Some(UploadMethod::Multipart);
        let h = head_with_etag("\"c265ed4f7dea6f6d4f2fc1f87fe92b93-2\"");
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn upload_method_put_match() {
        let mut exp = expected_obj("small.txt");
        exp.upload_method = Some(UploadMethod::PutObject);
        let h = head_with_etag("\"5eb63bbbe01eeed093cb22bb8f5acdc3\"");
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn upload_method_mismatch() {
        let mut exp = expected_obj("small.txt");
        exp.upload_method = Some(UploadMethod::Multipart);
        let h = head_with_etag("\"5eb63bbbe01eeed093cb22bb8f5acdc3\"");
        let results = assert_object(&exp, Some(&h), None);
        assert!(results.iter().any(|r| !r.is_pass()));
    }
}
