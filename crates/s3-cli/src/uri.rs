//! S3 URI parsing and path type validation.
//!
//! Handles the `s3://bucket/key` format used by the AWS CLI and provides
//! [`TransferUri`] for type-safe distinction between local and S3 paths.

use std::path::PathBuf;
use std::str::FromStr;

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
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("s3://") {
            Ok(TransferUri::S3(S3Uri::parse(s)))
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
                if uri.key.is_empty() {
                    write!(f, "s3://{}", uri.bucket)
                } else {
                    write!(f, "s3://{}/{}", uri.bucket, uri.key)
                }
            }
        }
    }
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
}
