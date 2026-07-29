//! node:process — `kill(pid, signal)`. Signal 0 is the existence check (no
//! signal sent); a non-zero signal terminates the target. Real syscalls
//! (POSIX `kill`, Win32 `OpenProcess`/`TerminateProcess`). Returns `true` on
//! success (process exists / signal delivered), `false` otherwise.

/// `process.kill(pid, signal)`.
#[rtse::function(module = "node:process", value = "kill")]
fn kill(pid: i64, signal: i64) -> bool {
    #[cfg(unix)]
    {
        (unsafe { libc::kill(pid as libc::pid_t, signal as i32) } == 0)
    }
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
            fn TerminateProcess(handle: isize, exit_code: u32) -> i32;
            fn CloseHandle(handle: isize) -> i32;
        }
        const PROCESS_TERMINATE: u32 = 0x0001;
        const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
        let access = if signal == 0 { PROCESS_QUERY_INFORMATION } else { PROCESS_TERMINATE };
        unsafe {
            let h = OpenProcess(access, 0, pid as u32);
            if h == 0 {
                return false;
            }
            let ok = if signal == 0 { 1 } else { TerminateProcess(h, 1) };
            CloseHandle(h);
            ok != 0
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, signal);
        false
    }
}
