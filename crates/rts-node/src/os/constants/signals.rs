//! `os.constants.signals` — POSIX signal numbers (from `libc`, real per-target)
//! or the fixed Windows `<signal.h>`/libuv set. Absent signals are omitted.

/// `(name, number)` pairs for every signal present on the compiled platform.
pub fn entries() -> Vec<(&'static str, i64)> {
    #[cfg(unix)]
    {
        unix_entries()
    }
    #[cfg(windows)]
    {
        windows_entries()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Vec::new()
    }
}

#[cfg(unix)]
fn unix_entries() -> Vec<(&'static str, i64)> {
    // Signals present across Linux/macOS/BSD.
    let mut v: Vec<(&'static str, i64)> = vec![
        ("SIGHUP", libc::SIGHUP as i64),
        ("SIGINT", libc::SIGINT as i64),
        ("SIGQUIT", libc::SIGQUIT as i64),
        ("SIGILL", libc::SIGILL as i64),
        ("SIGTRAP", libc::SIGTRAP as i64),
        ("SIGABRT", libc::SIGABRT as i64),
        ("SIGIOT", libc::SIGIOT as i64),
        ("SIGBUS", libc::SIGBUS as i64),
        ("SIGFPE", libc::SIGFPE as i64),
        ("SIGKILL", libc::SIGKILL as i64),
        ("SIGUSR1", libc::SIGUSR1 as i64),
        ("SIGUSR2", libc::SIGUSR2 as i64),
        ("SIGSEGV", libc::SIGSEGV as i64),
        ("SIGPIPE", libc::SIGPIPE as i64),
        ("SIGALRM", libc::SIGALRM as i64),
        ("SIGTERM", libc::SIGTERM as i64),
        ("SIGCHLD", libc::SIGCHLD as i64),
        ("SIGCONT", libc::SIGCONT as i64),
        ("SIGSTOP", libc::SIGSTOP as i64),
        ("SIGTSTP", libc::SIGTSTP as i64),
        ("SIGTTIN", libc::SIGTTIN as i64),
        ("SIGTTOU", libc::SIGTTOU as i64),
        ("SIGURG", libc::SIGURG as i64),
        ("SIGXCPU", libc::SIGXCPU as i64),
        ("SIGXFSZ", libc::SIGXFSZ as i64),
        ("SIGVTALRM", libc::SIGVTALRM as i64),
        ("SIGPROF", libc::SIGPROF as i64),
        ("SIGWINCH", libc::SIGWINCH as i64),
        ("SIGIO", libc::SIGIO as i64),
        ("SIGSYS", libc::SIGSYS as i64),
    ];

    #[cfg(target_os = "linux")]
    {
        v.push(("SIGSTKFLT", libc::SIGSTKFLT as i64));
        v.push(("SIGPWR", libc::SIGPWR as i64));
        v.push(("SIGPOLL", libc::SIGPOLL as i64));
    }
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    {
        v.push(("SIGINFO", libc::SIGINFO as i64));
        v.push(("SIGEMT", libc::SIGEMT as i64));
    }
    v
}

#[cfg(windows)]
fn windows_entries() -> Vec<(&'static str, i64)> {
    // Fixed Windows/libuv signal numbers (<signal.h> + libuv extensions).
    vec![
        ("SIGHUP", 1),
        ("SIGINT", 2),
        ("SIGILL", 4),
        ("SIGABRT_COMPAT", 6),
        ("SIGFPE", 8),
        ("SIGKILL", 9),
        ("SIGSEGV", 11),
        ("SIGTERM", 15),
        ("SIGBREAK", 21),
        ("SIGABRT", 22),
        ("SIGWINCH", 28),
    ]
}
