//! Assertion engine for comparing CLI output against expected values.
//!
//! Pure functions — no state, no I/O. Each match mode from the spec
//! format gets its own function.
//!
//! Submodules handle post-command state verification:
//! - [`object`] — S3 object assertions (HeadObject + body)
//! - [`file`] — local file assertions

pub mod file;
pub mod object;

use crate::spec::OutputAssertion;

/// Result of an assertion check.
#[derive(Debug)]
pub enum AssertionResult {
    /// Assertion passed.
    Pass,
    /// Assertion failed with a descriptive message.
    Fail { message: String },
}

impl AssertionResult {
    /// Returns true if the assertion passed.
    pub fn is_pass(&self) -> bool {
        matches!(self, AssertionResult::Pass)
    }
}

/// Dispatches to the right match mode based on the [`OutputAssertion`] variant.
pub fn assert_output(actual: &str, assertion: &OutputAssertion) -> AssertionResult {
    match assertion {
        OutputAssertion::Exact(expected) => assert_exact(actual, expected),
        OutputAssertion::Contains(needles) => assert_contains(actual, needles),
        OutputAssertion::Regex(patterns) => assert_regex(actual, patterns),
        OutputAssertion::Unordered(lines) => assert_unordered(actual, lines),
        OutputAssertion::Golden => AssertionResult::Fail {
            message: "golden file comparison not yet implemented".into(),
        },
    }
}

/// Normalizes a string by converting `\r\n` to `\n` and stripping trailing
/// whitespace from each line.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compares `actual` against `expected` after normalizing line endings and
/// trailing whitespace. On mismatch, produces a unified diff.
pub fn assert_exact(actual: &str, expected: &str) -> AssertionResult {
    let norm_actual = normalize(actual);
    let norm_expected = normalize(expected);
    if norm_actual == norm_expected {
        return AssertionResult::Pass;
    }
    let diff = similar::TextDiff::from_lines(&norm_expected, &norm_actual);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header("expected", "actual")
        .to_string();
    AssertionResult::Fail { message: unified }
}

/// Checks that every needle in `needles` appears as a substring of `actual`.
pub fn assert_contains(actual: &str, needles: &[String]) -> AssertionResult {
    let missing: Vec<&str> = needles
        .iter()
        .filter(|n| !actual.contains(n.as_str()))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        AssertionResult::Pass
    } else {
        AssertionResult::Fail {
            message: format!("missing expected substrings:\n{}", missing.join("\n")),
        }
    }
}

/// Splits output into terminal-rendered lines, treating `\r` as a line
/// separator in addition to `\n` and `\r\n`. The CLI's progress indicator
/// emits `\r` to overwrite the progress line with the final result; when
/// captured (not rendered on a TTY), both segments are present in the
/// buffer. This lets regex/unordered assertions match each segment
/// independently. Empty segments are filtered.
pub fn terminal_lines(s: &str) -> Vec<&str> {
    s.split(['\r', '\n'])
        .map(|l| l.trim_end())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Checks that every regex pattern matches at least one line of `actual`.
pub fn assert_regex(actual: &str, patterns: &[String]) -> AssertionResult {
    let lines = terminal_lines(actual);
    let mut unmatched = Vec::new();
    for pat in patterns {
        let re = match regex::Regex::new(pat) {
            Ok(r) => r,
            Err(e) => {
                return AssertionResult::Fail {
                    message: format!("regex compile error for pattern \"{pat}\": {e}"),
                };
            }
        };
        if !lines.iter().any(|line| re.is_match(line)) {
            unmatched.push(pat.as_str());
        }
    }
    if unmatched.is_empty() {
        AssertionResult::Pass
    } else {
        AssertionResult::Fail {
            message: format!("unmatched patterns:\n{}", unmatched.join("\n")),
        }
    }
}

/// Compares `actual` lines against `expected_lines` in sorted order,
/// ignoring empty lines in `actual`.
pub fn assert_unordered(actual: &str, expected_lines: &[String]) -> AssertionResult {
    let mut actual_sorted: Vec<&str> = terminal_lines(actual);
    let mut expected_sorted: Vec<&str> = expected_lines.iter().map(String::as_str).collect();
    actual_sorted.sort();
    expected_sorted.sort();
    if actual_sorted == expected_sorted {
        return AssertionResult::Pass;
    }
    let missing: Vec<&&str> = expected_sorted
        .iter()
        .filter(|l| !actual_sorted.contains(l))
        .collect();
    let extra: Vec<&&str> = actual_sorted
        .iter()
        .filter(|l| !expected_sorted.contains(l))
        .collect();
    let mut msg = String::new();
    if !missing.is_empty() {
        msg.push_str(&format!(
            "missing lines:\n{}\n",
            missing
                .iter()
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !extra.is_empty() {
        msg.push_str(&format!(
            "extra lines:\n{}",
            extra
                .iter()
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    AssertionResult::Fail {
        message: msg.trim().to_string(),
    }
}

/// Checks that `actual` equals `expected`.
pub fn assert_exit_code(actual: i32, expected: i32) -> AssertionResult {
    if actual == expected {
        AssertionResult::Pass
    } else {
        AssertionResult::Fail {
            message: format!("expected exit code {expected}, got {actual}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let result = assert_exact("hello\nworld\n", "hello\nworld\n");
        assert!(result.is_pass());
    }

    #[test]
    fn test_exact_trailing_whitespace() {
        let result = assert_exact("hello  \n", "hello\n");
        assert!(result.is_pass());
    }

    #[test]
    fn test_exact_line_ending_normalization() {
        let result = assert_exact("hello\r\nworld\r\n", "hello\nworld\n");
        assert!(result.is_pass());
    }

    #[test]
    fn test_exact_mismatch_with_diff() {
        let result = assert_exact("hello\n", "goodbye\n");
        assert!(!result.is_pass());
        let AssertionResult::Fail { message } = result else {
            panic!("expected Fail");
        };
        assert!(
            message.contains("---"),
            "diff should contain ---: {message}"
        );
        assert!(
            message.contains("+++"),
            "diff should contain +++: {message}"
        );
    }

    #[test]
    fn test_contains_all_present() {
        let actual = "the quick brown fox jumps over the lazy dog";
        let needles = vec!["quick".into(), "fox".into(), "lazy".into()];
        assert!(assert_contains(actual, &needles).is_pass());
    }

    #[test]
    fn test_contains_missing() {
        let actual = "the quick brown fox";
        let needles = vec!["quick".into(), "elephant".into()];
        let result = assert_contains(actual, &needles);
        assert!(!result.is_pass());
        let AssertionResult::Fail { message } = result else {
            panic!("expected Fail");
        };
        assert!(
            message.contains("elephant"),
            "should mention missing needle: {message}"
        );
    }

    #[test]
    fn test_regex_all_match() {
        let actual = "file1.txt 1024\nfile2.txt 2048\n";
        let patterns = vec![r"file1\.txt\s+\d+".into(), r"file2".into()];
        assert!(assert_regex(actual, &patterns).is_pass());
    }

    #[test]
    fn test_regex_unmatched() {
        let actual = "hello world\n";
        let patterns = vec![r"hello".into(), r"^goodbye$".into()];
        let result = assert_regex(actual, &patterns);
        assert!(!result.is_pass());
        let AssertionResult::Fail { message } = result else {
            panic!("expected Fail");
        };
        assert!(
            message.contains("^goodbye$"),
            "should mention unmatched pattern: {message}"
        );
    }

    #[test]
    fn test_regex_invalid_pattern() {
        let actual = "hello\n";
        let patterns = vec![r"[invalid".into()];
        let result = assert_regex(actual, &patterns);
        assert!(!result.is_pass());
        let AssertionResult::Fail { message } = result else {
            panic!("expected Fail");
        };
        assert!(
            message.contains("compile error"),
            "should mention compile error: {message}"
        );
    }

    #[test]
    fn test_unordered_same_lines_different_order() {
        let actual = "bravo\nalpha\ncharlie\n";
        let expected = vec!["alpha".into(), "bravo".into(), "charlie".into()];
        assert!(assert_unordered(actual, &expected).is_pass());
    }

    #[test]
    fn test_unordered_missing_line() {
        let actual = "alpha\nbravo\n";
        let expected = vec!["alpha".into(), "bravo".into(), "charlie".into()];
        let result = assert_unordered(actual, &expected);
        assert!(!result.is_pass());
        let AssertionResult::Fail { message } = result else {
            panic!("expected Fail");
        };
        assert!(
            message.contains("charlie"),
            "should mention missing line: {message}"
        );
    }

    #[test]
    fn test_regex_splits_on_carriage_return() {
        // CLI progress + final, separated by \r (overwritten on a real terminal)
        let actual = "Completed 11 Bytes/11 Bytes (5.1 KiB/s) with 1 file(s) remaining\rupload: ./a.txt to s3://b/a.txt\n";
        let patterns = vec![
            r"^Completed 11 Bytes/11 Bytes \([^)]+\) with 1 file\(s\) remaining$".into(),
            r"^upload: \./a\.txt to s3://b/a\.txt$".into(),
        ];
        assert!(assert_regex(actual, &patterns).is_pass());
    }

    #[test]
    fn test_exit_code_match() {
        assert!(assert_exit_code(0, 0).is_pass());
    }

    #[test]
    fn test_exit_code_mismatch() {
        let result = assert_exit_code(1, 0);
        assert!(!result.is_pass());
        let AssertionResult::Fail { message } = result else {
            panic!("expected Fail");
        };
        assert!(
            message.contains("0"),
            "should contain expected code: {message}"
        );
        assert!(
            message.contains("1"),
            "should contain actual code: {message}"
        );
    }

    #[test]
    fn test_assert_output_dispatches() {
        let assertion = OutputAssertion::Exact("hello\n".into());
        let result = assert_output("hello\n", &assertion);
        assert!(result.is_pass());
    }
}
