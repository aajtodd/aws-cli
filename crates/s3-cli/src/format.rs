//! Output formatting utilities shared across S3 CLI commands.
//!
//! Date and size formatting functions that match the Python AWS CLI's
//! output format exactly. Test vectors are ported from
//! `tests/unit/customizations/s3/test_utils.py`.

/// Format a smithy DateTime as `YYYY-MM-DD HH:MM:SS` in the system's local timezone.
///
/// Matches the Python CLI's `_make_last_mod_str` (subcommands.py:920).
/// Returns a 19-character string, or 19 spaces if the input is None or
/// conversion fails.
pub fn format_datetime_local(dt: Option<&aws_smithy_types::DateTime>) -> String {
    let Some(dt) = dt else {
        return " ".repeat(19);
    };
    let ts = match jiff::Timestamp::from_second(dt.secs()) {
        Ok(ts) => ts,
        Err(_) => return " ".repeat(19),
    };
    let local = ts.to_zoned(jiff::tz::TimeZone::system());
    local.strftime("%Y-%m-%d %H:%M:%S").to_string()
}

/// Format a byte size as a human-readable string.
///
/// Matches the Python CLI's `human_readable_size` (utils.py:69).
/// Uses base-2 units (KiB, MiB, etc.).
///
/// ```
/// use s3_cli::format::human_readable_size;
/// assert_eq!(human_readable_size(1), "1 Byte");
/// assert_eq!(human_readable_size(10), "10 Bytes");
/// assert_eq!(human_readable_size(1024), "1.0 KiB");
/// ```
pub fn human_readable_size(size: u64) -> String {
    if size == 1 {
        return "1 Byte".to_string();
    }
    if size < 1024 {
        return format!("{size} Bytes");
    }
    const SUFFIXES: &[&str] = &["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let base: f64 = 1024.0;
    let size_f = size as f64;
    for (i, suffix) in SUFFIXES.iter().enumerate() {
        let unit = base.powi(i as i32 + 2);
        if ((size_f / unit) * base).round() < base {
            return format!("{:.1} {suffix}", base * size_f / unit);
        }
    }
    // Fallback for extremely large values
    let unit = base.powi(SUFFIXES.len() as i32 + 1);
    format!("{:.1} EiB", base * size_f / unit)
}

/// Format a size value right-justified in a 10-character field.
///
/// If `human_readable` is true, uses [`human_readable_size`].
/// Otherwise formats as a plain integer.
pub fn format_size(size: i64, human_readable: bool) -> String {
    if human_readable {
        format!("{:>10}", human_readable_size(size as u64))
    } else {
        format!("{size:>10}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // human_readable_size — test vectors ported from Python CLI
    // tests/unit/customizations/s3/test_utils.py::test_human_readable_size
    // -----------------------------------------------------------------------

    #[test]
    fn human_readable_1_byte_singular() {
        assert_eq!(human_readable_size(1), "1 Byte");
    }

    #[test]
    fn human_readable_10_bytes_plural() {
        assert_eq!(human_readable_size(10), "10 Bytes");
    }

    #[test]
    fn human_readable_1000_bytes() {
        assert_eq!(human_readable_size(1000), "1000 Bytes");
    }

    #[test]
    fn human_readable_1_kib() {
        assert_eq!(human_readable_size(1024), "1.0 KiB");
    }

    #[test]
    fn human_readable_1_mib() {
        assert_eq!(human_readable_size(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn human_readable_1_gib() {
        assert_eq!(human_readable_size(1024 * 1024 * 1024), "1.0 GiB");
    }

    #[test]
    fn human_readable_1_tib() {
        assert_eq!(human_readable_size(1024u64.pow(4)), "1.0 TiB");
    }

    #[test]
    fn human_readable_1_pib() {
        assert_eq!(human_readable_size(1024u64.pow(5)), "1.0 PiB");
    }

    #[test]
    fn human_readable_1_eib() {
        assert_eq!(human_readable_size(1024u64.pow(6)), "1.0 EiB");
    }

    #[test]
    fn human_readable_just_under_1_mib() {
        // 1024^2 - 1 rounds up to "1.0 MiB" in the Python CLI
        assert_eq!(human_readable_size(1024 * 1024 - 1), "1.0 MiB");
    }

    #[test]
    fn human_readable_just_under_1_gib() {
        // 1024^3 - 1 rounds up to "1.0 GiB" in the Python CLI
        assert_eq!(human_readable_size(1024 * 1024 * 1024 - 1), "1.0 GiB");
    }

    // -----------------------------------------------------------------------
    // human_readable_size — additional edge cases (no Python equivalent)
    // -----------------------------------------------------------------------

    #[test]
    fn human_readable_zero() {
        assert_eq!(human_readable_size(0), "0 Bytes");
    }

    // -----------------------------------------------------------------------
    // format_size
    // -----------------------------------------------------------------------

    #[test]
    fn format_size_plain_right_justified() {
        assert_eq!(format_size(100, false), "       100");
        assert_eq!(format_size(0, false), "         0");
    }

    #[test]
    fn format_size_human_readable_right_justified() {
        assert_eq!(format_size(1024, true), "   1.0 KiB");
    }

    // -----------------------------------------------------------------------
    // format_datetime_local
    // -----------------------------------------------------------------------

    #[test]
    fn format_datetime_none_returns_spaces() {
        assert_eq!(format_datetime_local(None), " ".repeat(19));
    }

    #[test]
    fn format_datetime_produces_19_chars() {
        let dt = aws_smithy_types::DateTime::from_secs(1389304549); // 2014-01-09T20:45:49Z
        let result = format_datetime_local(Some(&dt));
        assert_eq!(result.len(), 19);
    }

    #[test]
    fn format_datetime_matches_local_conversion() {
        // Port of test_ls_command.py pattern: construct a known UTC time,
        // convert to local via jiff, verify our function matches.
        let epoch_secs = 1389304549i64; // 2014-01-09T20:45:49Z
        let dt = aws_smithy_types::DateTime::from_secs(epoch_secs);

        // Expected: what jiff produces for local time
        let ts = jiff::Timestamp::from_second(epoch_secs).unwrap();
        let local = ts.to_zoned(jiff::tz::TimeZone::system());
        let expected = local.strftime("%Y-%m-%d %H:%M:%S").to_string();

        assert_eq!(format_datetime_local(Some(&dt)), expected);
    }

    #[test]
    fn format_datetime_format_structure() {
        // Verify the format is YYYY-MM-DD HH:MM:SS regardless of timezone
        let dt = aws_smithy_types::DateTime::from_secs(0);
        let result = format_datetime_local(Some(&dt));
        assert_eq!(&result[4..5], "-");
        assert_eq!(&result[7..8], "-");
        assert_eq!(&result[10..11], " ");
        assert_eq!(&result[13..14], ":");
        assert_eq!(&result[16..17], ":");
    }
}
