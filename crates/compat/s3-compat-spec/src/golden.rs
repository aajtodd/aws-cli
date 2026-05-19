//! Golden file I/O for capturing and comparing baseline CLI output.

use std::path::{Path, PathBuf};

use crate::assertions::{AssertionResult, assert_exact};
use crate::error::{Error, ErrorKind};

/// Replace `YYYY-MM-DD HH:MM:SS` timestamp patterns with `{timestamp}`.
///
/// Used during prod validation where `last_modified` can't be controlled
/// via PutObject. The pattern matches the format `aws s3 ls` uses.
pub fn normalize_timestamps_in(s: String) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap());
    RE.replace_all(&s, "{timestamp}").into_owned()
}

/// Derive the golden file path for a given spec path and output stream.
///
/// `spec_path = "specs/commands/ls/basic.toml"`, `stream = "stdout"`
/// → `"specs/commands/ls/basic.stdout.golden"`
pub fn golden_path(spec_path: &Path, stream: &str) -> PathBuf {
    spec_path
        .with_extension("")
        .with_extension(format!("{stream}.golden"))
}

/// Read a golden file. Returns `None` if the file doesn't exist.
pub fn read_golden(path: &Path) -> Result<Option<String>, Error> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::new(
            ErrorKind::Io,
            format!("{}: {e}", path.display()),
        )),
    }
}

/// Write a golden file. Creates parent directories if needed.
pub fn write_golden(path: &Path, content: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(path, content)
        .map_err(|e| Error::new(ErrorKind::Io, format!("{}: {e}", path.display())))
}

/// Assert actual output against a golden file.
///
/// - Golden file exists → compare via exact match.
/// - Golden file absent → assert actual is empty.
/// - If `normalize_timestamps` is true, replaces `YYYY-MM-DD HH:MM:SS` patterns
///   with `{timestamp}` in both golden and actual before comparing. Used during
///   prod validation where `last_modified` can't be controlled.
pub fn assert_golden(
    actual: &str,
    golden_file_path: &Path,
    normalize_timestamps: bool,
) -> Result<AssertionResult, Error> {
    match read_golden(golden_file_path)? {
        Some(expected) => {
            if normalize_timestamps {
                let norm_expected = normalize_timestamps_in(expected);
                let norm_actual = normalize_timestamps_in(actual.to_string());
                Ok(assert_exact(&norm_actual, &norm_expected))
            } else {
                Ok(assert_exact(actual, &expected))
            }
        }
        None => {
            if actual.is_empty() {
                Ok(AssertionResult::Pass)
            } else {
                Ok(AssertionResult::Fail {
                    message: format!(
                        "no golden file at {}, but output is non-empty ({} bytes). \
                         Run with COMPAT_MODE=capture to create it.",
                        golden_file_path.display(),
                        actual.len()
                    ),
                })
            }
        }
    }
}

/// Check that at least one golden file exists for a spec.
///
/// Called when the spec uses `mode = "golden"` to ensure capture has been done.
pub fn check_golden_files_exist(spec_path: &Path) -> Result<(), Error> {
    let stdout_path = golden_path(spec_path, "stdout");
    let stderr_path = golden_path(spec_path, "stderr");
    if !stdout_path.exists() && !stderr_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidSpec,
            format!(
                "no golden files found for {}. Run with COMPAT_MODE=capture to create them.",
                spec_path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_path_stdout() {
        let p = golden_path(Path::new("specs/commands/ls/basic.toml"), "stdout");
        assert_eq!(p, PathBuf::from("specs/commands/ls/basic.stdout.golden"));
    }

    #[test]
    fn golden_path_stderr() {
        let p = golden_path(Path::new("specs/commands/ls/basic.toml"), "stderr");
        assert_eq!(p, PathBuf::from("specs/commands/ls/basic.stderr.golden"));
    }

    #[test]
    fn read_golden_missing_returns_none() {
        let result = read_golden(Path::new("/nonexistent/file.golden")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn write_and_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stdout.golden");
        write_golden(&path, "hello world\n").unwrap();
        let content = read_golden(&path).unwrap().unwrap();
        assert_eq!(content, "hello world\n");
    }

    #[test]
    fn assert_golden_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stdout.golden");
        write_golden(&path, "expected output\n").unwrap();
        let result = assert_golden("expected output\n", &path, false).unwrap();
        assert!(result.is_pass());
    }

    #[test]
    fn assert_golden_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stdout.golden");
        write_golden(&path, "expected\n").unwrap();
        let result = assert_golden("actual\n", &path, false).unwrap();
        assert!(!result.is_pass());
    }

    #[test]
    fn assert_golden_missing_file_empty_output_passes() {
        let result =
            assert_golden("", Path::new("/nonexistent/test.stdout.golden"), false).unwrap();
        assert!(result.is_pass());
    }

    #[test]
    fn assert_golden_missing_file_nonempty_output_fails() {
        let result = assert_golden(
            "unexpected output",
            Path::new("/nonexistent/test.stdout.golden"),
            false,
        )
        .unwrap();
        assert!(!result.is_pass());
    }

    #[test]
    fn check_golden_files_exist_none_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");
        assert!(check_golden_files_exist(&spec_path).is_err());
    }

    #[test]
    fn check_golden_files_exist_stdout_only_ok() {
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");
        write_golden(&golden_path(&spec_path, "stdout"), "output\n").unwrap();
        assert!(check_golden_files_exist(&spec_path).is_ok());
    }

    #[test]
    fn check_golden_files_exist_stderr_only_ok() {
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");
        write_golden(&golden_path(&spec_path, "stderr"), "error\n").unwrap();
        assert!(check_golden_files_exist(&spec_path).is_ok());
    }

    #[test]
    fn assert_golden_stdout_match_stderr_absent_and_empty() {
        // Spec has golden stdout, no stderr golden file.
        // stdout matches, stderr is empty → both pass.
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");
        let stdout_golden = golden_path(&spec_path, "stdout");
        write_golden(&stdout_golden, "expected output\n").unwrap();

        // stdout matches golden
        let result = assert_golden("expected output\n", &stdout_golden, false).unwrap();
        assert!(result.is_pass());

        // stderr has no golden file, output is empty → pass
        let stderr_golden = golden_path(&spec_path, "stderr");
        let result = assert_golden("", &stderr_golden, false).unwrap();
        assert!(result.is_pass());
    }

    #[test]
    fn assert_golden_stdout_match_stderr_absent_but_nonempty() {
        // Spec has golden stdout, no stderr golden file.
        // stdout matches, but CLI produced unexpected stderr → fail.
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");
        let stdout_golden = golden_path(&spec_path, "stdout");
        write_golden(&stdout_golden, "expected output\n").unwrap();

        // stdout matches
        let result = assert_golden("expected output\n", &stdout_golden, false).unwrap();
        assert!(result.is_pass());

        // stderr has no golden file but output is non-empty → fail
        let stderr_golden = golden_path(&spec_path, "stderr");
        let result = assert_golden("unexpected error\n", &stderr_golden, false).unwrap();
        assert!(!result.is_pass());
    }

    #[test]
    fn capture_writes_only_nonempty() {
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");

        let stdout_path = golden_path(&spec_path, "stdout");
        let stderr_path = golden_path(&spec_path, "stderr");

        // Simulate capture: non-empty stdout, empty stderr
        let stdout = "some output\n";
        let stderr = "";

        if !stdout.is_empty() {
            write_golden(&stdout_path, stdout).unwrap();
        }
        if !stderr.is_empty() {
            write_golden(&stderr_path, stderr).unwrap();
        }

        // stdout golden written
        assert!(stdout_path.exists());
        assert_eq!(
            std::fs::read_to_string(&stdout_path).unwrap(),
            "some output\n"
        );

        // stderr golden NOT written (empty output)
        assert!(!stderr_path.exists());
    }

    #[test]
    fn capture_writes_both_when_nonempty() {
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("test.toml");

        let stdout_path = golden_path(&spec_path, "stdout");
        let stderr_path = golden_path(&spec_path, "stderr");

        let stdout = "output\n";
        let stderr = "warning\n";

        if !stdout.is_empty() {
            write_golden(&stdout_path, stdout).unwrap();
        }
        if !stderr.is_empty() {
            write_golden(&stderr_path, stderr).unwrap();
        }

        assert!(stdout_path.exists());
        assert!(stderr_path.exists());
        assert_eq!(std::fs::read_to_string(&stderr_path).unwrap(), "warning\n");
    }

    #[test]
    fn normalize_timestamps_replaces_dates() {
        let input =
            "2024-06-15 14:30:00       1024 file1.txt\n2024-06-15 14:31:00       2048 file2.txt\n"
                .to_string();
        let normalized = normalize_timestamps_in(input);
        assert_eq!(
            normalized,
            "{timestamp}       1024 file1.txt\n{timestamp}       2048 file2.txt\n"
        );
    }

    #[test]
    fn normalize_timestamps_no_timestamps() {
        let input = "                           PRE docs/\n".to_string();
        let normalized = normalize_timestamps_in(input);
        assert_eq!(normalized, "                           PRE docs/\n");
    }

    #[test]
    fn normalize_timestamps_mixed_content() {
        let input =
            "                           PRE photos/\n2024-06-15 10:03:00          4 root.txt\n"
                .to_string();
        let normalized = normalize_timestamps_in(input);
        assert_eq!(
            normalized,
            "                           PRE photos/\n{timestamp}          4 root.txt\n"
        );
    }

    #[test]
    fn assert_golden_with_timestamp_normalization() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stdout.golden");
        write_golden(&path, "2024-06-15 14:30:00       1024 file1.txt\n").unwrap();
        // Different timestamp but same structure — passes with normalization
        let result =
            assert_golden("2026-04-10 18:23:02       1024 file1.txt\n", &path, true).unwrap();
        assert!(result.is_pass());
    }

    #[test]
    fn assert_golden_with_timestamp_normalization_content_differs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stdout.golden");
        write_golden(&path, "2024-06-15 14:30:00       1024 file1.txt\n").unwrap();
        // Different size — fails even with normalization
        let result =
            assert_golden("2026-04-10 18:23:02       2048 file1.txt\n", &path, true).unwrap();
        assert!(!result.is_pass());
    }

    #[test]
    fn assert_golden_without_normalization_timestamp_differs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stdout.golden");
        write_golden(&path, "2024-06-15 14:30:00       1024 file1.txt\n").unwrap();
        // Different timestamp — fails without normalization
        let result =
            assert_golden("2026-04-10 18:23:02       1024 file1.txt\n", &path, false).unwrap();
        assert!(!result.is_pass());
    }
}
