use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};

/// A running process spawned by a `Kaos` environment.
#[derive(Debug)]
pub struct Process {
    pid: u32,
    stdout: Arc<std::sync::Mutex<Vec<u8>>>,
    stderr: Arc<std::sync::Mutex<Vec<u8>>>,
    stdout_rx: Arc<std::sync::Mutex<Option<mpsc::UnboundedReceiver<Vec<u8>>>>>,
    stderr_rx: Arc<std::sync::Mutex<Option<mpsc::UnboundedReceiver<Vec<u8>>>>>,
    stdin: Arc<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>>,
    exit_rx: watch::Receiver<Option<i32>>,
}

impl Process {
    /// OS process id.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Exit code if the process has already terminated.
    pub fn exit_code(&self) -> Option<i32> {
        *self.exit_rx.borrow()
    }

    /// Wait for the process to exit and return its exit code.
    pub async fn wait(&self) -> i32 {
        let mut rx = self.exit_rx.clone();
        loop {
            if let Some(code) = *rx.borrow() {
                return code;
            }
            if rx.changed().await.is_err() {
                return -1;
            }
        }
    }

    /// Return all stdout bytes captured so far.
    pub async fn stdout(&self) -> Vec<u8> {
        self.stdout.lock().unwrap().clone()
    }

    /// Return all stderr bytes captured so far.
    pub async fn stderr(&self) -> Vec<u8> {
        self.stderr.lock().unwrap().clone()
    }

    /// Return a stream that yields stdout data as it is captured.
    ///
    /// The stream consumes a single-use channel; calling this more than once
    /// for the same output will return an already-closed stream on subsequent
    /// calls. The snapshot methods [`Self::stdout`] / [`Self::stderr`] remain
    /// usable because they read from a separate shared buffer.
    pub fn stdout_stream(&self) -> ProcessStream {
        let rx = self.stdout_rx.lock().unwrap().take();
        match rx {
            Some(rx) => ProcessStream::new(rx),
            None => ProcessStream::closed(),
        }
    }

    /// Return a stream that yields stderr data as it is captured.
    pub fn stderr_stream(&self) -> ProcessStream {
        let rx = self.stderr_rx.lock().unwrap().take();
        match rx {
            Some(rx) => ProcessStream::new(rx),
            None => ProcessStream::closed(),
        }
    }

    /// Return a handle to the process's stdin pipe.
    ///
    /// The handle is `None` if stdin was not piped (should not happen for
    /// processes spawned through `Kaos`). Lock the mutex and write to the
    /// inner `tokio::process::ChildStdin` asynchronously.
    pub fn stdin(&self) -> Arc<tokio::sync::Mutex<Option<tokio::process::ChildStdin>>> {
        Arc::clone(&self.stdin)
    }

    /// Convenience: write the given bytes to stdin and flush.
    pub async fn write_stdin(&self, data: &[u8]) -> io::Result<()> {
        let mut guard = self.stdin.lock().await;
        if let Some(stdin) = guard.as_mut() {
            stdin.write_all(data).await?;
            stdin.flush().await?;
        }
        Ok(())
    }

    /// Close the process's stdin pipe.
    ///
    /// Useful for programs like `cat` that wait for EOF before exiting.
    pub async fn close_stdin(&self) -> io::Result<()> {
        let mut guard = self.stdin.lock().await;
        if guard.is_some() {
            guard.take();
        }
        Ok(())
    }

    /// Send a signal to the process (defaults to SIGTERM).
    /// On POSIX the whole process group is signalled.
    pub async fn kill(&self, signal: Option<&str>) -> io::Result<()> {
        // Guard against a pid that was never assigned. This should not happen
        // because spawn() returns an error if the child fails to start, but
        // the check prevents accidentally signalling process group -1 on POSIX.
        if self.pid == 0 {
            return Ok(());
        }

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, killpg, Signal};
            use nix::unistd::Pid;

            let sig = match signal {
                Some("SIGKILL") => Signal::SIGKILL,
                Some("SIGINT") => Signal::SIGINT,
                _ => Signal::SIGTERM,
            };

            match killpg(Pid::from_raw(self.pid as i32), sig) {
                Ok(()) => Ok(()),
                Err(nix::errno::Errno::ESRCH) => Ok(()), // already gone
                Err(nix::errno::Errno::EPERM) => {
                    // Fall back to signalling the direct child only.
                    match kill(Pid::from_raw(self.pid as i32), sig) {
                        Ok(()) => Ok(()),
                        Err(nix::errno::Errno::ESRCH) => Ok(()),
                        Err(e) => Err(io::Error::new(io::ErrorKind::PermissionDenied, e)),
                    }
                }
                Err(e) => Err(io::Error::other(e)),
            }
        }

        #[cfg(windows)]
        {
            // Mirror TS LocalProcess.kill(): taskkill /T kills the whole tree.
            // /F is added only for SIGKILL-equivalent force kill.
            let force = signal == Some("SIGKILL");
            let pid_str = self.pid.to_string();
            let args: Vec<&str> = if force {
                vec!["/T", "/F", "/PID", &pid_str]
            } else {
                vec!["/T", "/PID", &pid_str]
            };

            let mut child = tokio::process::Command::new("taskkill")
                .args(&args)
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn()?;
            child.wait().await?;
            Ok(())
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = signal;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "kill is not supported on this platform",
            ))
        }
    }
}

/// An asynchronous stream over one of a [`Process`]'s captured outputs.
pub struct ProcessStream {
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl ProcessStream {
    fn new(rx: mpsc::UnboundedReceiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buf: Vec::new(),
            pos: 0,
        }
    }

    fn closed() -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        drop(tx);
        Self::new(rx)
    }
}

impl AsyncRead for ProcessStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            if self.pos < self.buf.len() {
                let available = &self.buf[self.pos..];
                let n = std::cmp::min(buf.remaining(), available.len());
                buf.put_slice(&available[..n]);
                self.pos += n;
                return Poll::Ready(Ok(()));
            }

            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    self.buf = chunk;
                    self.pos = 0;
                    continue;
                }
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

pub(crate) async fn spawn(
    cwd: &Path,
    args: &[&str],
    env: Option<&[(&str, &str)]>,
) -> io::Result<Process> {
    if args.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "exec requires at least one argument",
        ));
    }

    let mut cmd = Command::new(args[0]);
    cmd.args(&args[1..]);
    cmd.current_dir(cwd);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(vars) = env {
        for (k, v) in vars {
            cmd.env(k, v);
        }
    }

    #[cfg(unix)]
    {
        // Make the child a process-group leader so POSIX kill can signal
        // the whole tree (direct child + grandchildren).
        cmd.process_group(0);
    }

    let mut child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let stdout_buf = Arc::new(std::sync::Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(std::sync::Mutex::new(Vec::new()));

    let (stdout_tx, stdout_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (stderr_tx, stderr_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let stdout_clone = Arc::clone(&stdout_buf);
    tokio::spawn(capture_stream(stdout, stdout_clone, stdout_tx));

    let stderr_clone = Arc::clone(&stderr_buf);
    tokio::spawn(capture_stream(stderr, stderr_clone, stderr_tx));

    let (exit_tx, exit_rx) = watch::channel(None);
    tokio::spawn(async move {
        let status = child.wait().await.ok();
        let code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        let _ = exit_tx.send(Some(code));
    });

    Ok(Process {
        pid,
        stdout: stdout_buf,
        stderr: stderr_buf,
        stdout_rx: Arc::new(std::sync::Mutex::new(Some(stdout_rx))),
        stderr_rx: Arc::new(std::sync::Mutex::new(Some(stderr_rx))),
        stdin: Arc::new(tokio::sync::Mutex::new(Some(stdin))),
        exit_rx,
    })
}

async fn capture_stream<R>(
    mut reader: R,
    buffer: Arc<std::sync::Mutex<Vec<u8>>>,
    tx: mpsc::UnboundedSender<Vec<u8>>,
) where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                let bytes = chunk[..n].to_vec();
                buffer.lock().unwrap().extend_from_slice(&bytes);
                let _ = tx.send(bytes);
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::environment::Environment;
    use crate::kaos::Kaos;
    use tokio::io::AsyncReadExt;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".to_string(),
            os_arch: "arm64".to_string(),
            os_version: "23.0.0".to_string(),
            shell_name: "bash".to_string(),
            shell_path: "/bin/bash".to_string(),
        }
    }

    #[tokio::test]
    async fn exec_echo() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos.exec(&["/bin/echo", "-n", "hello"]).await.unwrap();
        let code = proc.wait().await;
        let out = proc.stdout().await;
        assert_eq!(out, b"hello");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn exec_with_env_sees_cwd_and_variable() {
        let tmp = tempfile::tempdir().unwrap();
        let kaos = Kaos::new(dummy_env(), tmp.path()).with_cwd(tmp.path());

        let proc = kaos
            .exec_with_env(&["/bin/sh", "-c", "printf '%s' \"$PWD\""], &[])
            .await
            .unwrap();
        assert_eq!(proc.wait().await, 0);
        let out = String::from_utf8(proc.stdout().await).unwrap();
        // Canonicalize both paths (macOS /var → /private/var symlink)
        let out_canon = std::fs::canonicalize(&out).unwrap();
        let tmp_canon = std::fs::canonicalize(tmp.path()).unwrap();
        assert_eq!(out_canon, tmp_canon);

        let proc2 = kaos
            .exec_with_env(
                &["/bin/sh", "-c", "printf '%s' \"$MYVAR\""],
                &[("MYVAR", "bar")],
            )
            .await
            .unwrap();
        assert_eq!(proc2.wait().await, 0);
        assert_eq!(proc2.stdout().await, b"bar");
    }

    #[tokio::test]
    async fn wait_then_read_keeps_stdout_and_stderr() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos
            .exec(&["/bin/sh", "-c", "printf out; printf err >&2"])
            .await
            .unwrap();
        assert_eq!(proc.wait().await, 0);
        assert_eq!(proc.stdout().await, b"out");
        assert_eq!(proc.stderr().await, b"err");
    }

    #[tokio::test]
    async fn large_stdout_does_not_deadlock() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let n = 200_000;
        let proc = kaos
            .exec(&[
                "node",
                "-e",
                &format!("process.stdout.write('A'.repeat({}))", n),
            ])
            .await
            .unwrap();
        assert_eq!(proc.wait().await, 0);
        let out = proc.stdout().await;
        assert_eq!(out.len(), n);
        assert!(out.iter().all(|&b| b == b'A'));
    }

    #[tokio::test]
    async fn false_returns_one() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos.exec(&["/bin/sh", "-c", "false"]).await.unwrap();
        assert_eq!(proc.wait().await, 1);
        assert_eq!(proc.exit_code(), Some(1));
    }

    #[tokio::test]
    async fn custom_exit_code() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos.exec(&["/bin/sh", "-c", "exit 42"]).await.unwrap();
        assert_eq!(proc.wait().await, 42);
    }

    #[tokio::test]
    async fn missing_command_returns_not_found() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let err = kaos.exec(&["__missing_command_12345"]).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn kill_terminates_long_running_process() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos.exec(&["/bin/sleep", "30"]).await.unwrap();
        proc.kill(None).await.unwrap();
        let code = proc.wait().await;
        assert_ne!(code, 0); // killed by signal
    }

    #[tokio::test]
    async fn kill_tree_terminates_grandchildren() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("pids.txt");
        let marker_str = marker.to_string_lossy().to_string();
        let script = format!(
            "echo $$ >> {}; sleep 30 & echo $! >> {}; wait",
            marker_str, marker_str
        );
        let kaos = Kaos::new(dummy_env(), tmp.path());
        let proc = kaos.exec(&["/bin/sh", "-c", &script]).await.unwrap();

        // Give the shell time to fork the background sleep.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        proc.kill(None).await.unwrap();
        proc.wait().await;

        let content = tokio::fs::read_to_string(&marker).await.unwrap();
        for pid_str in content.split_whitespace() {
            let pid: i32 = pid_str.parse().unwrap();
            let still_running = std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            assert!(!still_running, "pid {} still running", pid);
        }
    }

    #[tokio::test]
    async fn write_to_stdin_is_received() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos.exec(&["/bin/cat"]).await.unwrap();
        proc.write_stdin(b"hello stdin").await.unwrap();
        // Close stdin so cat exits.
        {
            let stdin = proc.stdin();
            let mut guard = stdin.lock().await;
            guard.take();
        }
        assert_eq!(proc.wait().await, 0);
        assert_eq!(proc.stdout().await, b"hello stdin");
    }

    #[tokio::test]
    async fn stdout_stream_returns_all_data() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let proc = kaos
            .exec(&["/bin/sh", "-c", "printf 'line1\nline2\n'"])
            .await
            .unwrap();
        let mut stream = proc.stdout_stream();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"line1\nline2\n");
    }
}
