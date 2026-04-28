//! Local path formatting utilities.
//!
//! Matches the Python AWS CLI's path display behavior in
//! `awscli/customizations/s3/utils.py::relative_path()`.

use std::path::{Path, PathBuf};

/// Format a local path for display, relative to the current working directory.
///
/// Matches the Python CLI's `relative_path()` default invocation:
/// - Split path into dirname + basename
/// - Compute `relpath(dirname, cwd)` + `/basename`
/// - On error (e.g. cross-drive on Windows, CWD unavailable), fall back to
///   the path as given
pub fn format_local_path(path: &Path) -> String {
    match std::env::current_dir() {
        Ok(cwd) => relative_path(path, &cwd),
        Err(_) => path.display().to_string(),
    }
}

/// Compute `path` relative to `start`, or fall back to the original path string
/// if no relative path can be computed.
///
/// Mirrors Python's `relative_path(filename, start)`:
/// ```python
/// dirname, basename = os.path.split(filename)
/// relative_dir = os.path.relpath(dirname, start)
/// return os.path.join(relative_dir, basename)
/// ```
pub fn relative_path(path: &Path, start: &Path) -> String {
    let (dirname, basename) = split_path(path);
    match relpath_components(&dirname, start) {
        Some(rel) => {
            if let Some(base) = basename {
                rel.join(base).display().to_string()
            } else {
                rel.display().to_string()
            }
        }
        None => path.display().to_string(),
    }
}

fn split_path(path: &Path) -> (PathBuf, Option<&std::ffi::OsStr>) {
    let dirname = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let basename = path.file_name();
    (dirname, basename)
}

/// Compute `path` relative to `start` using path components.
/// Returns `None` if no relative path can be computed (e.g. different
/// Windows drive letters).
fn relpath_components(path: &Path, start: &Path) -> Option<PathBuf> {
    // Relative input paths are interpreted relative to `start`.
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        start.join(path)
    };
    if !start.is_absolute() {
        return None;
    }
    let start = start.to_path_buf();

    // On Windows, cross-drive paths have different prefixes — no relpath exists.
    #[cfg(windows)]
    {
        use std::path::Component;
        let path_prefix = path.components().next().and_then(|c| match c {
            Component::Prefix(p) => Some(p),
            _ => None,
        });
        let start_prefix = start.components().next().and_then(|c| match c {
            Component::Prefix(p) => Some(p),
            _ => None,
        });
        if path_prefix != start_prefix {
            return None;
        }
    }

    let path_comps: Vec<_> = path.components().collect();
    let start_comps: Vec<_> = start.components().collect();

    let common = path_comps
        .iter()
        .zip(start_comps.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let up_levels = start_comps.len() - common;
    let remaining = &path_comps[common..];

    if up_levels == 0 && remaining.is_empty() {
        return Some(PathBuf::from("."));
    }

    let mut result = PathBuf::new();
    for _ in 0..up_levels {
        result.push("..");
    }
    for c in remaining {
        result.push(c.as_os_str());
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sep() -> char {
        std::path::MAIN_SEPARATOR
    }

    /// Python: `relative_path('/tmp/foo/bar', '/tmp/foo')` →
    /// split → dirname='/tmp/foo', basename='bar'
    /// relpath('/tmp/foo', '/tmp/foo') = '.'
    /// join('.', 'bar') = './bar'
    #[test]
    fn relpath_subdir_file() {
        let result = relative_path(Path::new("/tmp/foo/bar"), Path::new("/tmp/foo"));
        assert_eq!(result, format!(".{}bar", sep()));
    }

    /// Python: `relative_path('/tmp/foo/bar', '/tmp/foo/bar')` →
    /// split → dirname='/tmp/foo', basename='bar'
    /// relpath('/tmp/foo', '/tmp/foo/bar') = '..'
    /// join('..', 'bar') = '../bar'
    #[test]
    fn relpath_same_path() {
        let result = relative_path(Path::new("/tmp/foo/bar"), Path::new("/tmp/foo/bar"));
        assert_eq!(result, format!("..{}bar", sep()));
    }

    /// Python: `relative_path('/etc/passwd', '/tmp')` →
    /// split → dirname='/etc', basename='passwd'
    /// relpath('/etc', '/tmp') = '../etc'
    /// join('../etc', 'passwd') = '../etc/passwd'
    #[test]
    fn relpath_sibling_tree() {
        let result = relative_path(Path::new("/etc/passwd"), Path::new("/tmp"));
        assert_eq!(result, format!("..{s}etc{s}passwd", s = sep()));
    }

    /// Python: `relative_path('/tmp/foo/bar/baz', '/tmp/foo')` →
    /// split → dirname='/tmp/foo/bar', basename='baz'
    /// relpath('/tmp/foo/bar', '/tmp/foo') = 'bar'
    /// join('bar', 'baz') = 'bar/baz'
    #[test]
    fn relpath_nested_subdir() {
        let result = relative_path(Path::new("/tmp/foo/bar/baz"), Path::new("/tmp/foo"));
        assert_eq!(result, format!("bar{}baz", sep()));
    }

    /// Python: `relative_path('/tmp/foo', '/tmp')` →
    /// split → dirname='/tmp', basename='foo'
    /// relpath('/tmp', '/tmp') = '.'
    /// join('.', 'foo') = './foo'
    #[test]
    fn relpath_child_of_cwd() {
        let result = relative_path(Path::new("/tmp/foo"), Path::new("/tmp"));
        assert_eq!(result, format!(".{}foo", sep()));
    }

    /// Python: `relative_path('/a/b/c/d/e', '/a/b/x/y')` →
    /// split → dirname='/a/b/c/d', basename='e'
    /// relpath('/a/b/c/d', '/a/b/x/y') = '../../c/d'
    /// join('../../c/d', 'e') = '../../c/d/e'
    #[test]
    fn relpath_deep_divergence() {
        let result = relative_path(Path::new("/a/b/c/d/e"), Path::new("/a/b/x/y"));
        assert_eq!(result, format!("..{s}..{s}c{s}d{s}e", s = sep()));
    }

    /// Relative input path — the behavior is CWD-dependent in Python, so we
    /// don't try to match it exactly. We verify our implementation handles it
    /// without panicking.
    #[test]
    fn relpath_relative_input_does_not_panic() {
        let _ = relative_path(Path::new("foo/bar"), Path::new("/tmp"));
    }

    /// Non-absolute `start` cannot produce a relpath — fall back to raw.
    #[test]
    fn relpath_non_absolute_start_falls_back() {
        let result = relative_path(Path::new("/tmp/foo"), Path::new("relative/start"));
        assert_eq!(result, "/tmp/foo");
    }

    /// `format_local_path` uses CWD. Smoke test: path inside CWD round-trips
    /// to something starting with `.` (the dirname relpath is `.`).
    #[test]
    fn format_local_path_smoke() {
        let cwd = std::env::current_dir().expect("cwd");
        let file = cwd.join("some-test-file.txt");
        let result = format_local_path(&file);
        assert_eq!(result, format!(".{}some-test-file.txt", sep()));
    }
}
