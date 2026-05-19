//! Local file state assertions.

use std::path::Path;

use super::AssertionResult;
use super::object::{byte_preview, crc32c_base64};
use crate::spec::ExpectedFile;

/// Assert a single expected file against filesystem state.
pub fn assert_file(expected: &ExpectedFile, working_dir: &Path) -> Vec<AssertionResult> {
    let mut results = Vec::new();
    let path = working_dir.join(&expected.path);
    let exists = path.exists();

    match (expected.exists, exists) {
        (true, false) => {
            results.push(AssertionResult::Fail {
                message: format!("file {:?} expected to exist but not found", expected.path),
            });
            return results;
        }
        (false, false) => {
            results.push(AssertionResult::Pass);
            return results;
        }
        (false, true) => {
            results.push(AssertionResult::Fail {
                message: format!("file {:?} expected to not exist but found", expected.path),
            });
            return results;
        }
        (true, true) => {
            results.push(AssertionResult::Pass);
        }
    }

    // Content
    if let Some(expected_content) = &expected.content {
        match std::fs::read(&path) {
            Ok(actual) => {
                if actual == expected_content.as_bytes() {
                    tracing::debug!(path = %expected.path, "file content matches");
                    results.push(AssertionResult::Pass);
                } else {
                    let expected_bytes = expected_content.as_bytes();
                    results.push(AssertionResult::Fail {
                        message: format!(
                            "file {:?} content mismatch:\n  \
                             expected ({} bytes, crc32c={}): {}\n  \
                             actual   ({} bytes, crc32c={}): {}",
                            expected.path,
                            expected_bytes.len(),
                            crc32c_base64(expected_bytes),
                            byte_preview(expected_bytes),
                            actual.len(),
                            crc32c_base64(&actual),
                            byte_preview(&actual),
                        ),
                    });
                }
            }
            Err(e) => {
                results.push(AssertionResult::Fail {
                    message: format!("file {:?} read error: {}", expected.path, e),
                });
            }
        }
    }

    // Size
    if let Some(expected_size) = expected.size {
        match std::fs::metadata(&path) {
            Ok(meta) => {
                let actual_size = meta.len();
                if actual_size == expected_size {
                    tracing::debug!(path = %expected.path, size = actual_size, "file size matches");
                    results.push(AssertionResult::Pass);
                } else {
                    results.push(AssertionResult::Fail {
                        message: format!(
                            "file {:?} size mismatch: expected {} bytes, got {} bytes",
                            expected.path, expected_size, actual_size
                        ),
                    });
                }
            }
            Err(e) => {
                results.push(AssertionResult::Fail {
                    message: format!("file {:?} metadata error: {}", expected.path, e),
                });
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_exists_pass() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: true,
            content: None,
            size: None,
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn file_exists_but_missing() {
        let dir = tempfile::tempdir().unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: true,
            content: None,
            size: None,
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn file_not_exists_pass() {
        let dir = tempfile::tempdir().unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: false,
            content: None,
            size: None,
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn file_not_exists_but_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: false,
            content: None,
            size: None,
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn file_content_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: true,
            content: Some("hello".into()),
            size: None,
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn file_content_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "world").unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: true,
            content: Some("hello".into()),
            size: None,
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().any(|r| !r.is_pass()));
    }

    #[test]
    fn file_size_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: true,
            content: None,
            size: Some(5),
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().all(|r| r.is_pass()));
    }

    #[test]
    fn file_size_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
        let exp = ExpectedFile {
            path: "f.txt".into(),
            exists: true,
            content: None,
            size: Some(99),
        };
        let results = assert_file(&exp, dir.path());
        assert!(results.iter().any(|r| !r.is_pass()));
    }
}
