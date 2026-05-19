use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::process::Command;

use crate::error::{Error, ErrorKind};

/// Output captured from a CLI subprocess execution.
#[derive(Debug)]
pub struct CliOutput {
    /// Process exit code (1 if terminated by signal without a code).
    pub exit_code: i32,
    /// Complete stdout captured from the process.
    pub stdout: String,
    /// Complete stderr captured from the process.
    pub stderr: String,
    /// Wall-clock time from spawn to exit.
    pub duration: Duration,
}

/// Runs a CLI binary as a subprocess with environment isolation, stdin piping,
/// and timeout support.
pub struct CliExecutor {
    /// Path to the CLI binary to execute.
    binary_path: PathBuf,
    /// Timeout applied when no per-call timeout is specified.
    default_timeout: Duration,
}

impl CliExecutor {
    pub fn new(binary_path: impl Into<PathBuf>, default_timeout: Duration) -> Self {
        Self {
            binary_path: binary_path.into(),
            default_timeout,
        }
    }

    /// Execute the CLI binary with the given arguments, environment, working
    /// directory, optional stdin, and optional per-call timeout.
    ///
    /// The subprocess environment is fully isolated: `env_clear()` is called
    /// before injecting only the provided `env` vars.
    pub async fn execute(
        &self,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: &Path,
        stdin: Option<&str>,
        timeout: Option<Duration>,
    ) -> Result<CliOutput, Error> {
        if !self.binary_path.exists() {
            return Err(Error::new(
                ErrorKind::BinaryNotFound,
                format!("path: {}", self.binary_path.display()),
            ));
        }

        let mut cmd = Command::new(&self.binary_path);
        cmd.args(args)
            .env_clear()
            .envs(env)
            .current_dir(working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if stdin.is_some() {
            cmd.stdin(std::process::Stdio::piped());
        }

        let timeout_dur = timeout.unwrap_or(self.default_timeout);
        let start = Instant::now();
        let mut child = cmd.spawn()?;

        if let Some(input) = stdin {
            use tokio::io::AsyncWriteExt;
            let mut child_stdin = child.stdin.take().expect("stdin was piped");
            child_stdin.write_all(input.as_bytes()).await?;
            drop(child_stdin); // close stdin so the child sees EOF
        }

        let output = tokio::time::timeout(timeout_dur, child.wait_with_output()).await;

        match output {
            Ok(Ok(output)) => Ok(CliOutput {
                exit_code: output.status.code().unwrap_or(1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                duration: start.elapsed(),
            }),
            Ok(Err(e)) => Err(Error::new(ErrorKind::Io, e)),
            Err(_) => {
                // child was consumed by wait_with_output, but the timeout
                // means the future was dropped, which drops the child and
                // kills the process automatically in tokio.
                Err(Error::new(ErrorKind::Timeout, format!("{timeout_dur:?}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_executor(binary: &str) -> CliExecutor {
        CliExecutor::new(binary, Duration::from_secs(5))
    }

    fn empty_env() -> HashMap<String, String> {
        HashMap::new()
    }

    fn tmp() -> PathBuf {
        std::env::temp_dir()
    }

    fn find_binary(name: &str) -> PathBuf {
        let bin = PathBuf::from(format!("/bin/{name}"));
        if bin.exists() {
            return bin;
        }
        let usr_bin = PathBuf::from(format!("/usr/bin/{name}"));
        if usr_bin.exists() {
            return usr_bin;
        }
        bin
    }

    #[cfg_attr(windows, ignore)]
    #[tokio::test]
    async fn test_execute_echo() {
        let exec = make_executor(find_binary("echo").to_str().unwrap());
        let out = exec
            .execute(&[String::from("hello")], &empty_env(), &tmp(), None, None)
            .await
            .unwrap();
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "hello\n");
        assert!(out.stderr.is_empty());
    }

    #[cfg_attr(windows, ignore)]
    #[tokio::test]
    async fn test_execute_stderr() {
        let exec = make_executor(find_binary("sh").to_str().unwrap());
        let out = exec
            .execute(
                &[String::from("-c"), String::from("echo error >&2")],
                &empty_env(),
                &tmp(),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(out.stderr, "error\n");
        assert!(out.stdout.is_empty());
    }

    #[cfg_attr(windows, ignore)]
    #[tokio::test]
    async fn test_execute_nonzero_exit() {
        let exec = make_executor(find_binary("sh").to_str().unwrap());
        let out = exec
            .execute(
                &[String::from("-c"), String::from("exit 42")],
                &empty_env(),
                &tmp(),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(out.exit_code, 42);
    }

    #[cfg_attr(windows, ignore)]
    #[tokio::test]
    async fn test_execute_stdin() {
        let exec = make_executor(find_binary("cat").to_str().unwrap());
        let out = exec
            .execute(&[], &empty_env(), &tmp(), Some("hello from stdin"), None)
            .await
            .unwrap();
        assert_eq!(out.stdout, "hello from stdin");
    }

    #[cfg_attr(windows, ignore)]
    #[tokio::test]
    async fn test_execute_timeout() {
        let exec = make_executor(find_binary("sleep").to_str().unwrap());
        let result = exec
            .execute(
                &[String::from("10")],
                &empty_env(),
                &tmp(),
                None,
                Some(Duration::from_millis(100)),
            )
            .await;
        assert_eq!(result.unwrap_err().kind(), &ErrorKind::Timeout);
    }

    #[cfg_attr(windows, ignore)]
    #[tokio::test]
    async fn test_execute_env_isolation() {
        let exec = make_executor(find_binary("sh").to_str().unwrap());
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/test/home"));
        let out = exec
            .execute(
                &[String::from("-c"), String::from("echo $HOME")],
                &env,
                &tmp(),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(out.stdout, "/test/home\n");
    }

    #[tokio::test]
    async fn test_execute_binary_not_found() {
        let exec = make_executor("/nonexistent/binary/path");
        let result = exec.execute(&[], &empty_env(), &tmp(), None, None).await;
        assert_eq!(result.unwrap_err().kind(), &ErrorKind::BinaryNotFound);
    }
}
