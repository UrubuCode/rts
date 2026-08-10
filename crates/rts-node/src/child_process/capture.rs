//! Runs a `Command` to completion on the CALLING thread, capturing stdout
//! and stderr — the piece [`super::sync_ops`]'s three members share.
//!
//! # Why not `Command::output()`
//!
//! `output()` cannot write `input` to the child's stdin AND does not let a
//! `timeout` interrupt it mid-run — it blocks until the child exits, however
//! long that is. This spawns the child itself, hands `input` to its stdin on
//! two background threads that also drain stdout/stderr concurrently (so a
//! child that fills a pipe buffer while this thread is polling `try_wait`
//! never deadlocks), and polls `try_wait` so a `timeout` can `kill()` the
//! child instead of waiting forever.
//!
//! # Signals: named, not delivered
//!
//! `Child::kill()` — the only termination `std` exposes with no extra crate
//! — sends `SIGKILL` on POSIX and calls `TerminateProcess` on Windows.
//! Neither is `killSignal`'s configured signal (`SIGTERM` by default): this
//! module cannot send an arbitrary POSIX signal without `libc`, which is not
//! a dependency this module may add (it may only edit its own folder, not
//! `Cargo.toml`). So every kill this module performs — on `timeout` or on
//! `maxBuffer` — is reported as `"SIGKILL"`, honestly, rather than echoing
//! back the `killSignal` name nothing here actually sent.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// What one run answered.
pub(super) struct Capture {
    pub(super) pid: Option<u32>,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) status: Option<i32>,
    pub(super) signal: Option<&'static str>,
    /// `(message, code)` — set only when the process never started, or when
    /// `maxBuffer` was exceeded.
    pub(super) error: Option<(String, &'static str)>,
}

/// Runs `command`, feeding it `input` (if any) and capturing both streams,
/// killing it if `timeout_ms` elapses or combined output exceeds
/// `max_buffer`.
pub(super) fn run(mut command: Command, input: Option<Vec<u8>>, timeout_ms: Option<f64>, max_buffer: usize) -> Capture {
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Capture {
                pid: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
                status: None,
                signal: None,
                error: Some((error.to_string(), spawn_error_code(&error))),
            };
        }
    };
    let pid = child.id();
    if let Some(bytes) = input
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(&bytes);
        // Dropped here (end of scope) to send EOF — the child sees a closed
        // stdin exactly once the write above returns.
    }
    let out_rx = drain(child.stdout.take());
    let err_rx = drain(child.stderr.take());

    let start = Instant::now();
    let mut killed = false;
    let mut status = None;
    loop {
        match child.try_wait() {
            Ok(Some(found)) => {
                status = Some(found);
                break;
            }
            Ok(None) => {
                let over_time = timeout_ms.is_some_and(|ms| start.elapsed().as_secs_f64() * 1000.0 >= ms);
                if over_time {
                    let _ = child.kill();
                    killed = true;
                    status = child.wait().ok();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
    let mut stdout = out_rx.recv().unwrap_or_default();
    let mut stderr = err_rx.recv().unwrap_or_default();
    let mut over_buffer = false;
    if stdout.len() + stderr.len() > max_buffer {
        over_buffer = true;
        // Combined-length truncation, split proportionally — no ceiling one
        // stream could starve the other of, and Node itself does not
        // document a byte-exact split policy either.
        let half = max_buffer / 2;
        stdout.truncate(half.min(stdout.len()));
        stderr.truncate((max_buffer - stdout.len()).min(stderr.len()));
    }
    let signal = match killed {
        true => Some("SIGKILL"),
        false => status.and_then(signal_of),
    };
    let error = if over_buffer {
        Some(("maxBuffer exceeded".to_owned(), "ERR_CHILD_PROCESS_STDIO_MAXBUFFER"))
    } else {
        None
    };
    Capture { pid: Some(pid), stdout, stderr, status: status.and_then(|s| s.code()), signal, error }
}

/// Reads a pipe to completion on its own thread, handing the bytes back over
/// a channel — draining stdout/stderr concurrently with the `try_wait` poll
/// loop above is what keeps a chatty child from blocking on a full pipe
/// while this thread is busy sleeping between polls.
fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    match pipe {
        Some(mut reader) => {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = reader.read_to_end(&mut bytes);
                let _ = tx.send(bytes);
            });
        }
        None => {
            let _ = tx.send(Vec::new());
        }
    }
    rx
}

/// `ENOENT` for "not found", `EACCES` for "found, not permitted to run" —
/// the two spawn failures a program actually branches on, per the reference
/// doc's §4. Anything else answers `"UNKNOWN"` rather than a fabricated
/// specific code.
fn spawn_error_code(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "ENOENT",
        std::io::ErrorKind::PermissionDenied => "EACCES",
        _ => "UNKNOWN",
    }
}

/// The signal that ended a process, by name — POSIX only. `std`'s
/// `ExitStatusExt::signal()` reads this straight out of the OS wait status,
/// needing no `libc` dependency; the number→name table is this module's own
/// small, independent copy (per the reference doc's §5.1: not borrowed from
/// `rts-std`).
#[cfg(unix)]
pub(super) fn signal_of(status: std::process::ExitStatus) -> Option<&'static str> {
    use std::os::unix::process::ExitStatusExt;
    match status.signal()? {
        1 => Some("SIGHUP"),
        2 => Some("SIGINT"),
        3 => Some("SIGQUIT"),
        6 => Some("SIGABRT"),
        9 => Some("SIGKILL"),
        11 => Some("SIGSEGV"),
        13 => Some("SIGPIPE"),
        15 => Some("SIGTERM"),
        _ => None,
    }
}

/// Windows has no POSIX signal to report — an exit that was not a normal
/// `ExitCode` reads as `None` here rather than a guessed name.
#[cfg(not(unix))]
pub(super) fn signal_of(_status: std::process::ExitStatus) -> Option<&'static str> {
    None
}
