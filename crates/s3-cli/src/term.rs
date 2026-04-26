//! Terminal abstraction for output and terminal control.
//!
//! [`Terminal`] models a terminal with stdout/stderr streams and
//! terminal-level operations (TTY detection, width, line clearing).
//! [`TermOutput`] models a single output stream.
//!
//! Production code uses [`StdTerminal`]. Tests use [`InMemoryTerminal`]
//! which captures output through a vt100 parser for accurate assertions
//! including `\r` overwrites and ANSI escape processing.

use std::io::{self, IsTerminal, Write};

/// A single output stream (stdout or stderr).
pub trait TermOutput: Send + Sync {
    /// Write a string without a trailing newline.
    fn write(&self, s: &str) -> io::Result<()>;

    /// Write a string followed by a newline.
    fn writeln(&self, s: &str) -> io::Result<()>;
}

/// A terminal with stdout/stderr streams and terminal-level operations.
pub trait Terminal: Send + Sync {
    /// The stdout stream.
    fn out(&self) -> &dyn TermOutput;

    /// The stderr stream.
    fn err(&self) -> &dyn TermOutput;

    /// Whether stdout is connected to a TTY.
    fn is_tty(&self) -> bool;

    /// Terminal width in columns.
    fn width(&self) -> u16;
}

/// Write formatted output to a terminal's stdout.
#[macro_export]
macro_rules! termout {
    ($term:expr, $($arg:tt)*) => {
        $term.out().write(&format!($($arg)*))
    };
}

/// Write formatted output to a terminal's stdout, followed by a newline.
#[macro_export]
macro_rules! termoutln {
    ($term:expr) => { $term.out().writeln("") };
    ($term:expr, $($arg:tt)*) => {
        $term.out().writeln(&format!($($arg)*))
    };
}

/// Write formatted output to a terminal's stderr.
#[macro_export]
macro_rules! termerr {
    ($term:expr, $($arg:tt)*) => {
        $term.err().write(&format!($($arg)*))
    };
}

/// Write formatted output to a terminal's stderr, followed by a newline.
#[macro_export]
macro_rules! termerrln {
    ($term:expr, $($arg:tt)*) => {
        $term.err().writeln(&format!($($arg)*))
    };
}

// ---------------------------------------------------------------------------
// Production implementation
// ---------------------------------------------------------------------------

/// Terminal connected to real stdout/stderr.
pub struct StdTerminal {
    out: StdOut,
    err: StdErr,
    is_tty: bool,
}

impl StdTerminal {
    /// Create a terminal connected to the process's stdout/stderr.
    pub fn new() -> Self {
        Self {
            out: StdOut,
            err: StdErr,
            is_tty: io::stdout().is_terminal(),
        }
    }
}

impl Default for StdTerminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal for StdTerminal {
    fn out(&self) -> &dyn TermOutput {
        &self.out
    }
    fn err(&self) -> &dyn TermOutput {
        &self.err
    }
    fn is_tty(&self) -> bool {
        self.is_tty
    }
    fn width(&self) -> u16 {
        80 // TODO: detect via console crate
    }
}

struct StdOut;
struct StdErr;

impl TermOutput for StdOut {
    fn write(&self, s: &str) -> io::Result<()> {
        io::stdout().lock().write_all(s.as_bytes())
    }
    fn writeln(&self, s: &str) -> io::Result<()> {
        let mut out = io::stdout().lock();
        out.write_all(s.as_bytes())?;
        out.write_all(b"\n")
    }
}

impl TermOutput for StdErr {
    fn write(&self, s: &str) -> io::Result<()> {
        io::stderr().lock().write_all(s.as_bytes())
    }
    fn writeln(&self, s: &str) -> io::Result<()> {
        let mut out = io::stderr().lock();
        out.write_all(s.as_bytes())?;
        out.write_all(b"\n")
    }
}

// ---------------------------------------------------------------------------
// Test implementation
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A terminal that captures output in memory via vt100 parsers.
    ///
    /// Clone to share between test setup and assertions.
    #[derive(Clone)]
    pub struct InMemoryTerminal {
        out: InMemoryOutput,
        err: InMemoryOutput,
        width: u16,
    }

    impl InMemoryTerminal {
        /// Create a new in-memory terminal with the given dimensions.
        pub fn new(rows: u16, cols: u16) -> Self {
            Self {
                out: InMemoryOutput::new(rows, cols),
                err: InMemoryOutput::new(rows, cols),
                width: cols,
            }
        }

        /// What a user would see on stdout (after `\r` overwrites, escapes, etc.).
        pub fn stdout_contents(&self) -> String {
            self.out.contents()
        }

        /// What a user would see on stderr.
        pub fn stderr_contents(&self) -> String {
            self.err.contents()
        }
    }

    impl Terminal for InMemoryTerminal {
        fn out(&self) -> &dyn TermOutput {
            &self.out
        }
        fn err(&self) -> &dyn TermOutput {
            &self.err
        }
        fn is_tty(&self) -> bool {
            false
        }
        fn width(&self) -> u16 {
            self.width
        }
    }

    /// A single output stream backed by a vt100 parser.
    #[derive(Clone)]
    struct InMemoryOutput(Arc<Mutex<vt100::Parser>>);

    impl InMemoryOutput {
        fn new(rows: u16, cols: u16) -> Self {
            Self(Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0))))
        }

        fn contents(&self) -> String {
            let parser = self.0.lock().unwrap();
            let mut rows: Vec<String> = parser.screen().rows(0, parser.screen().size().1).collect();

            // Trim trailing empty rows.
            while rows.last().is_some_and(|r| r.trim_end().is_empty()) {
                rows.pop();
            }

            // Trim trailing whitespace per row.
            rows.iter()
                .map(|r| r.trim_end())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    impl TermOutput for InMemoryOutput {
        fn write(&self, s: &str) -> io::Result<()> {
            self.0.lock().unwrap().process(s.as_bytes());
            Ok(())
        }

        fn writeln(&self, s: &str) -> io::Result<()> {
            let mut parser = self.0.lock().unwrap();
            parser.process(s.as_bytes());
            parser.process(b"\r\n");
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn basic_write_line() {
            let t = InMemoryTerminal::new(10, 80);
            t.out().writeln("hello").unwrap();
            t.out().writeln("world").unwrap();
            assert_eq!(t.stdout_contents(), "hello\nworld");
        }

        #[test]
        fn write_without_newline() {
            let t = InMemoryTerminal::new(10, 80);
            t.out().write("partial").unwrap();
            assert_eq!(t.stdout_contents(), "partial");
        }

        #[test]
        fn carriage_return_overwrites() {
            let t = InMemoryTerminal::new(10, 80);
            t.out().write("hello\rworld").unwrap();
            assert_eq!(t.stdout_contents(), "world");
        }

        #[test]
        fn separate_stdout_stderr() {
            let t = InMemoryTerminal::new(10, 80);
            t.out().writeln("out").unwrap();
            t.err().writeln("err").unwrap();
            assert_eq!(t.stdout_contents(), "out");
            assert_eq!(t.stderr_contents(), "err");
        }

        #[test]
        fn clone_shares_state() {
            let t1 = InMemoryTerminal::new(10, 80);
            let t2 = t1.clone();
            t1.out().writeln("from t1").unwrap();
            assert_eq!(t2.stdout_contents(), "from t1");
        }

        #[test]
        fn empty_terminal() {
            let t = InMemoryTerminal::new(10, 80);
            assert_eq!(t.stdout_contents(), "");
            assert_eq!(t.stderr_contents(), "");
        }

        #[test]
        fn width_returns_configured_value() {
            let t = InMemoryTerminal::new(10, 120);
            assert_eq!(t.width(), 120);
        }

        #[test]
        fn is_tty_returns_false() {
            let t = InMemoryTerminal::new(10, 80);
            assert!(!t.is_tty());
        }

        #[test]
        fn empty_writeln() {
            let t = InMemoryTerminal::new(10, 80);
            t.out().writeln("first").unwrap();
            t.out().writeln("").unwrap();
            t.out().writeln("third").unwrap();
            assert_eq!(t.stdout_contents(), "first\n\nthird");
        }
    }
}
