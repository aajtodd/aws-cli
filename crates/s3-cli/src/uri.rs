//! S3 URI parsing and path type validation.
//!
//! Handles the `s3://bucket/key` format and S3 access point ARNs used by the
//! AWS CLI. Provides [`TransferUri`] for type-safe distinction between local
//! and S3 paths.

use std::path::PathBuf;
use std::str::FromStr;

use crate::arn::Arn;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UriParseError {
    #[error("s3 commands do not support S3 Object Lambda resources. Use s3api commands instead.")]
    UnsupportedObjectLambda,
    #[error("s3 commands do not support Outpost Bucket ARNs. Use s3control commands instead.")]
    UnsupportedOutpostBucket,
}

/// A parsed S3 URI (`s3://bucket/key`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Uri {
    /// The bucket name.
    pub bucket: String,
    /// The key (prefix). Empty string if no key was specified.
    pub key: String,
}

impl S3Uri {
    /// Parse from a string that has already been confirmed to start with `s3://`.
    fn parse(s: &str) -> Self {
        let path = s.strip_prefix("s3://").unwrap_or(s);
        let (bucket, key) = match path.find('/') {
            Some(idx) => (path[..idx].to_string(), path[idx + 1..].to_string()),
            None => (path.to_string(), String::new()),
        };
        S3Uri { bucket, key }
    }
}

/// A transfer source or destination — either a local path or an S3 URI.
#[derive(Debug, Clone)]
pub enum TransferUri {
    /// A local filesystem path.
    Local(PathBuf),
    /// An S3 URI (`s3://bucket/key`).
    S3(S3Uri),
}

impl FromStr for TransferUri {
    type Err = UriParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("s3://") {
            Ok(TransferUri::S3(S3Uri::parse(s)))
        } else if s.starts_with("arn:") {
            parse_arn_uri(s)
        } else {
            Ok(TransferUri::Local(PathBuf::from(s)))
        }
    }
}

impl std::fmt::Display for TransferUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferUri::Local(p) => write!(f, "{}", p.display()),
            TransferUri::S3(uri) => {
                let is_arn = uri.bucket.starts_with("arn:");
                if uri.key.is_empty() {
                    if is_arn {
                        write!(f, "{}", uri.bucket)
                    } else {
                        write!(f, "s3://{}", uri.bucket)
                    }
                } else if is_arn {
                    write!(f, "{}/{}", uri.bucket, uri.key)
                } else {
                    write!(f, "s3://{}/{}", uri.bucket, uri.key)
                }
            }
        }
    }
}

/// Parse an `arn:` prefixed string into a `TransferUri`.
///
/// Rejects unsupported ARN shapes (Object Lambda, Outposts bucket) with the
/// exact error messages Python uses. Malformed ARNs fall through to `Local`.
fn parse_arn_uri(s: &str) -> Result<TransferUri, UriParseError> {
    let arn = match Arn::parse(s) {
        Ok(a) => a,
        Err(_) => return Ok(TransferUri::Local(PathBuf::from(s))),
    };

    if arn.service == "s3-object-lambda" {
        return Err(UriParseError::UnsupportedObjectLambda);
    }

    if arn.service == "s3" {
        // Standard access point: resource = accesspoint[/:]NAME[/KEY...]
        if let Some(rest) = arn
            .resource
            .strip_prefix("accesspoint/")
            .or_else(|| arn.resource.strip_prefix("accesspoint:"))
        {
            let (bucket, key) = split_ap_name_key(rest, &arn.resource, s);
            return Ok(TransferUri::S3(S3Uri { bucket, key }));
        }
    } else if arn.service == "s3-outposts" {
        // Outposts resource: outpost[/:]ID[/:]accesspoint[/:]NAME or outpost[/:]ID[/:]bucket[/:]NAME
        if let Some(after_outpost) = strip_segment_prefix(&arn.resource, "outpost") {
            if let Some(after_id) = after_outpost.split_once(['/', ':']) {
                let (_, rest_after_id) = after_id;
                if let Some(ap_rest) = strip_segment_prefix(rest_after_id, "accesspoint") {
                    let (bucket, key) = split_ap_name_key(ap_rest, &arn.resource, s);
                    return Ok(TransferUri::S3(S3Uri { bucket, key }));
                }
                if strip_segment_prefix(rest_after_id, "bucket").is_some() {
                    return Err(UriParseError::UnsupportedOutpostBucket);
                }
            }
        }
    }

    // Unknown shape — fall through to Local (Python's behavior).
    Ok(TransferUri::Local(PathBuf::from(s)))
}

/// Strip a segment prefix like `"accesspoint"` followed by `/` or `:`, returning
/// the remainder. Returns `None` if the string doesn't start with `segment[/:]`.
fn strip_segment_prefix<'a>(s: &'a str, segment: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(segment)?;
    rest.strip_prefix('/').or_else(|| rest.strip_prefix(':'))
}

/// Given the text after `accesspoint[/:]`, split into (full_arn_bucket, key).
///
/// `ap_rest` is the text after the `accesspoint/` or `accesspoint:` prefix.
/// `resource` is the full resource field. `original` is the full input string.
fn split_ap_name_key(ap_rest: &str, resource: &str, original: &str) -> (String, String) {
    let (_, key) = match ap_rest.find('/') {
        Some(idx) => (&ap_rest[..idx], &ap_rest[idx + 1..]),
        None => (ap_rest, ""),
    };
    // Bucket = everything in the original up to and including the AP name.
    // The resource field starts at the 6th colon-separated field of the ARN.
    // We find where the key starts in the resource and take the original up to that point.
    let name_end_in_resource = resource.len() - if key.is_empty() { 0 } else { key.len() + 1 };
    let resource_start = original.len() - resource.len();
    let bucket = &original[..resource_start + name_end_in_resource];
    (bucket.to_string(), key.to_string())
}

/// The type of transfer determined by source and destination paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathType {
    LocalToS3,
    S3ToLocal,
    S3ToS3,
}

/// Determine the path type from source and destination.
pub fn path_type(src: &TransferUri, dest: &TransferUri) -> Option<PathType> {
    match (src, dest) {
        (TransferUri::Local(_), TransferUri::S3(_)) => Some(PathType::LocalToS3),
        (TransferUri::S3(_), TransferUri::Local(_)) => Some(PathType::S3ToLocal),
        (TransferUri::S3(_), TransferUri::S3(_)) => Some(PathType::S3ToS3),
        (TransferUri::Local(_), TransferUri::Local(_)) => None,
    }
}

/// Validate that a command supports the given path type.
///
/// Returns an error message matching the Python CLI format if invalid.
pub fn validate_path_type(cmd: &str, paths: &[TransferUri]) -> Result<(), String> {
    let valid = match paths.len() {
        1 => matches!(paths[0], TransferUri::S3(_)) && matches!(cmd, "mb" | "rb" | "rm"),
        2 => {
            let pt = path_type(&paths[0], &paths[1]);
            match pt {
                Some(_) => matches!(cmd, "cp" | "mv" | "sync"),
                None => false, // local-to-local
            }
        }
        _ => false,
    };

    if valid {
        Ok(())
    } else {
        let usage = match cmd {
            "cp" => "<LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>",
            "mv" => "<LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>",
            "sync" => "<LocalPath> <S3Uri> or <S3Uri> <LocalPath> or <S3Uri> <S3Uri>",
            "rm" => "<S3Uri>",
            "mb" => "<S3Uri>",
            "rb" => "<S3Uri>",
            _ => "<S3Uri>",
        };
        Err(format!(
            "usage: aws s3 {cmd} {usage}\nError: Invalid argument type"
        ))
    }
}

/// Parse an S3 URI of the form `s3://bucket[/key]`.
///
/// Returns `None` if the input doesn't start with `s3://`.
///
/// # Examples
///
/// ```
/// use s3_cli::uri::parse_s3_uri;
///
/// let uri = parse_s3_uri("s3://my-bucket/path/to/key").unwrap();
/// assert_eq!(uri.bucket, "my-bucket");
/// assert_eq!(uri.key, "path/to/key");
///
/// assert!(parse_s3_uri("/local/path").is_none());
/// ```
pub fn parse_s3_uri(uri: &str) -> Option<S3Uri> {
    if uri.starts_with("s3://") {
        Some(S3Uri::parse(uri))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- S3Uri parsing ---

    #[test]
    fn bare_s3_prefix() {
        let uri = parse_s3_uri("s3://").unwrap();
        assert_eq!(uri.bucket, "");
        assert_eq!(uri.key, "");
    }

    #[test]
    fn bucket_only() {
        let uri = parse_s3_uri("s3://my-bucket").unwrap();
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "");
    }

    #[test]
    fn bucket_with_trailing_slash() {
        let uri = parse_s3_uri("s3://my-bucket/").unwrap();
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "");
    }

    #[test]
    fn bucket_and_key() {
        let uri = parse_s3_uri("s3://my-bucket/some/key").unwrap();
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "some/key");
    }

    #[test]
    fn bucket_and_key_with_trailing_slash() {
        let uri = parse_s3_uri("s3://my-bucket/prefix/").unwrap();
        assert_eq!(uri.bucket, "my-bucket");
        assert_eq!(uri.key, "prefix/");
    }

    #[test]
    fn not_s3_uri() {
        assert!(parse_s3_uri("/local/path").is_none());
        assert!(parse_s3_uri("./relative").is_none());
        assert!(parse_s3_uri("http://example.com").is_none());
        assert!(parse_s3_uri("s3:missing-slashes").is_none());
    }

    #[test]
    fn key_with_special_chars() {
        let uri = parse_s3_uri("s3://bucket/key with spaces/file.txt").unwrap();
        assert_eq!(uri.bucket, "bucket");
        assert_eq!(uri.key, "key with spaces/file.txt");
    }

    #[test]
    fn key_with_unicode() {
        let uri = parse_s3_uri("s3://bucket/日本語/ファイル.txt").unwrap();
        assert_eq!(uri.bucket, "bucket");
        assert_eq!(uri.key, "日本語/ファイル.txt");
    }

    // --- TransferUri ---

    #[test]
    fn transfer_uri_s3() {
        let uri: TransferUri = "s3://bucket/key".parse().unwrap();
        assert!(matches!(uri, TransferUri::S3(ref u) if u.bucket == "bucket" && u.key == "key"));
    }

    #[test]
    fn transfer_uri_local() {
        let uri: TransferUri = "/tmp/file.txt".parse().unwrap();
        assert!(matches!(uri, TransferUri::Local(ref p) if p == &PathBuf::from("/tmp/file.txt")));
    }

    #[test]
    fn transfer_uri_display() {
        let s3: TransferUri = "s3://bucket/key".parse().unwrap();
        assert_eq!(s3.to_string(), "s3://bucket/key");
        let local: TransferUri = "/tmp/file".parse().unwrap();
        assert_eq!(local.to_string(), "/tmp/file");
    }

    // --- Path type validation ---
    // Mirrors Python CLI's test_check_path_type_pass / test_check_path_type_fail

    fn s3(s: &str) -> TransferUri {
        TransferUri::S3(S3Uri::parse(s))
    }

    fn local(s: &str) -> TransferUri {
        TransferUri::Local(PathBuf::from(s))
    }

    #[test]
    fn path_type_valid_two_path_commands() {
        for cmd in ["cp", "mv", "sync"] {
            assert!(validate_path_type(cmd, &[local("/tmp"), s3("s3://b")]).is_ok());
            assert!(validate_path_type(cmd, &[s3("s3://b"), local("/tmp")]).is_ok());
            assert!(validate_path_type(cmd, &[s3("s3://a"), s3("s3://b")]).is_ok());
        }
    }

    #[test]
    fn path_type_valid_single_path_commands() {
        for cmd in ["mb", "rb", "rm"] {
            assert!(validate_path_type(cmd, &[s3("s3://bucket")]).is_ok());
        }
    }

    #[test]
    fn path_type_local_to_local_always_invalid() {
        for cmd in ["cp", "mv", "sync"] {
            let err = validate_path_type(cmd, &[local("/a"), local("/b")]).unwrap_err();
            assert!(err.contains("Invalid argument type"));
        }
    }

    #[test]
    fn path_type_local_single_always_invalid() {
        for cmd in ["mb", "rb", "rm"] {
            let err = validate_path_type(cmd, &[local("/tmp")]).unwrap_err();
            assert!(err.contains("Invalid argument type"));
        }
    }

    #[test]
    fn path_type_wrong_command_for_type() {
        // Two-path commands reject single S3 path
        assert!(validate_path_type("cp", &[s3("s3://b")]).is_err());
        // Single-path commands reject two paths
        assert!(validate_path_type("mb", &[s3("s3://a"), s3("s3://b")]).is_err());
    }

    // Exhaustive invalid matrix — ported from Python CLI's test_check_path_type_fail.
    // Each command rejects all path types it doesn't support.

    #[test]
    fn path_type_cp_rejects_invalid() {
        assert!(validate_path_type("cp", &[local("/a")]).is_err()); // local only
        assert!(validate_path_type("cp", &[local("/a"), local("/b")]).is_err()); // local-local
        assert!(validate_path_type("cp", &[s3("s3://b")]).is_err()); // s3 only
    }

    #[test]
    fn path_type_mv_rejects_invalid() {
        assert!(validate_path_type("mv", &[local("/a")]).is_err());
        assert!(validate_path_type("mv", &[local("/a"), local("/b")]).is_err());
        assert!(validate_path_type("mv", &[s3("s3://b")]).is_err());
    }

    #[test]
    fn path_type_sync_rejects_invalid() {
        assert!(validate_path_type("sync", &[local("/a")]).is_err());
        assert!(validate_path_type("sync", &[local("/a"), local("/b")]).is_err());
        assert!(validate_path_type("sync", &[s3("s3://b")]).is_err());
    }

    #[test]
    fn path_type_rm_rejects_invalid() {
        assert!(validate_path_type("rm", &[local("/a")]).is_err());
        assert!(validate_path_type("rm", &[local("/a"), local("/b")]).is_err());
        assert!(validate_path_type("rm", &[s3("s3://a"), s3("s3://b")]).is_err());
        assert!(validate_path_type("rm", &[local("/a"), s3("s3://b")]).is_err());
        assert!(validate_path_type("rm", &[s3("s3://b"), local("/a")]).is_err());
    }

    #[test]
    fn path_type_mb_rejects_invalid() {
        assert!(validate_path_type("mb", &[local("/a")]).is_err());
        assert!(validate_path_type("mb", &[local("/a"), local("/b")]).is_err());
        assert!(validate_path_type("mb", &[s3("s3://a"), s3("s3://b")]).is_err());
        assert!(validate_path_type("mb", &[local("/a"), s3("s3://b")]).is_err());
        assert!(validate_path_type("mb", &[s3("s3://b"), local("/a")]).is_err());
    }

    #[test]
    fn path_type_rb_rejects_invalid() {
        assert!(validate_path_type("rb", &[local("/a")]).is_err());
        assert!(validate_path_type("rb", &[local("/a"), local("/b")]).is_err());
        assert!(validate_path_type("rb", &[s3("s3://a"), s3("s3://b")]).is_err());
        assert!(validate_path_type("rb", &[local("/a"), s3("s3://b")]).is_err());
        assert!(validate_path_type("rb", &[s3("s3://b"), local("/a")]).is_err());
    }

    #[test]
    fn path_type_error_format_matches_cli() {
        let err = validate_path_type("cp", &[local("/a"), local("/b")]).unwrap_err();
        assert!(err.starts_with("usage: aws s3 cp "));
        assert!(err.ends_with("Error: Invalid argument type"));
    }

    // --- ARN parsing via TransferUri::from_str ---

    #[test]
    fn arn_standard_access_point_no_key() {
        let uri: TransferUri = "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap"
            .parse()
            .unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap"
        );
        assert_eq!(s3.key, "");
    }

    #[test]
    fn arn_standard_access_point_with_key() {
        let uri: TransferUri = "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap/dir/file.txt"
            .parse()
            .unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap"
        );
        assert_eq!(s3.key, "dir/file.txt");
    }

    #[test]
    fn arn_standard_access_point_colon_separator() {
        let uri: TransferUri = "arn:aws:s3:us-east-1:123456789012:accesspoint:my-ap"
            .parse()
            .unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert!(s3.bucket.starts_with("arn:aws:s3:"));
        assert!(s3.bucket.ends_with("my-ap"));
        assert_eq!(s3.key, "");
    }

    #[test]
    fn arn_mrap() {
        let uri: TransferUri = "arn:aws:s3::123456789012:accesspoint/mfzwi23gnjvgw.mrap/foo.txt"
            .parse()
            .unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3::123456789012:accesspoint/mfzwi23gnjvgw.mrap"
        );
        assert_eq!(s3.key, "foo.txt");
    }

    #[test]
    fn arn_partition_variants() {
        for part in ["aws", "aws-cn", "aws-us-gov", "aws-iso", "aws-iso-b"] {
            let uri: TransferUri = format!("arn:{part}:s3:us-east-1:123456789012:accesspoint/ap")
                .parse()
                .unwrap();
            let TransferUri::S3(s3) = uri else {
                panic!("expected S3")
            };
            assert_eq!(s3.key, "");
            assert!(s3.bucket.contains(part));
        }
    }

    #[test]
    fn arn_outposts_access_point_no_key() {
        let uri: TransferUri =
            "arn:aws:s3-outposts:us-east-1:123456789012:outpost/op-0123abcd/accesspoint/my-ap"
                .parse()
                .unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3-outposts:us-east-1:123456789012:outpost/op-0123abcd/accesspoint/my-ap"
        );
        assert_eq!(s3.key, "");
    }

    #[test]
    fn arn_outposts_access_point_with_key() {
        let uri: TransferUri = "arn:aws:s3-outposts:us-east-1:123456789012:outpost/op-0123abcd/accesspoint/my-ap/data/file.bin"
            .parse()
            .unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3-outposts:us-east-1:123456789012:outpost/op-0123abcd/accesspoint/my-ap"
        );
        assert_eq!(s3.key, "data/file.bin");
    }

    #[test]
    fn arn_object_lambda_rejected() {
        let err = "arn:aws:s3-object-lambda:us-east-1:123456789012:accesspoint/my-olap"
            .parse::<TransferUri>()
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "s3 commands do not support S3 Object Lambda resources. Use s3api commands instead."
        );
    }

    #[test]
    fn arn_outposts_bucket_rejected() {
        let err = "arn:aws:s3-outposts:us-east-1:123456789012:outpost/op-0123abcd/bucket/my-bucket"
            .parse::<TransferUri>()
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "s3 commands do not support Outpost Bucket ARNs. Use s3control commands instead."
        );
    }

    #[test]
    fn arn_display_roundtrip_no_key() {
        let input = "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap";
        let uri: TransferUri = input.parse().unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn arn_display_roundtrip_with_key() {
        let input = "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap/dir/file.txt";
        let uri: TransferUri = input.parse().unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn arn_display_outposts_roundtrip() {
        let input = "arn:aws:s3-outposts:us-east-1:123456789012:outpost/op-0123abcd/accesspoint/my-ap/data.bin";
        let uri: TransferUri = input.parse().unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn malformed_arn_falls_through_to_local() {
        let uri: TransferUri = "arn:broken".parse().unwrap();
        assert!(matches!(uri, TransferUri::Local(_)));
    }

    #[test]
    fn existing_s3_uri_unaffected() {
        let uri: TransferUri = "s3://bucket/key".parse().unwrap();
        let TransferUri::S3(s3) = uri else { panic!() };
        assert_eq!(s3.bucket, "bucket");
        assert_eq!(s3.key, "key");
    }

    #[test]
    fn existing_local_path_unaffected() {
        let uri: TransferUri = "/tmp/file.txt".parse().unwrap();
        assert!(matches!(uri, TransferUri::Local(_)));
    }

    #[test]
    fn arn_standard_access_point_deeply_nested_key() {
        let input = "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap/a/b/c/d/e/deep.txt";
        let uri: TransferUri = input.parse().unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap"
        );
        assert_eq!(s3.key, "a/b/c/d/e/deep.txt");
    }

    #[test]
    fn arn_standard_access_point_colon_separator_with_key() {
        // The `:` form must still correctly split the key. The `/` after
        // the AP name is what terminates the name.
        let input = "arn:aws:s3:us-east-1:123456789012:accesspoint:my-ap/dir/file.txt";
        let uri: TransferUri = input.parse().unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(s3.key, "dir/file.txt");
        assert!(s3.bucket.ends_with("my-ap"));
    }

    #[test]
    fn arn_mrap_with_deeply_nested_key() {
        let input = "arn:aws:s3::123456789012:accesspoint/mfzwi23gnjvgw.mrap/a/b/c.txt";
        let uri: TransferUri = input.parse().unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert_eq!(
            s3.bucket,
            "arn:aws:s3::123456789012:accesspoint/mfzwi23gnjvgw.mrap"
        );
        assert_eq!(s3.key, "a/b/c.txt");
    }

    #[test]
    fn arn_outposts_all_colon_separators() {
        // Python's regex uses `[/:]` throughout Outposts parsing; the all-`:`
        // form must be accepted.
        let input = "arn:aws:s3-outposts:us-east-1:123456789012:outpost:op-0123:accesspoint:my-ap";
        let uri: TransferUri = input.parse().unwrap();
        let TransferUri::S3(s3) = uri else {
            panic!("expected S3")
        };
        assert!(s3.bucket.contains("outpost:op-0123"));
        assert!(s3.bucket.ends_with("my-ap"));
        assert_eq!(s3.key, "");
    }
}
