//! node:os — `os.getPriority([pid])` / `os.setPriority([pid,] priority)`.
//!
//! A single `-20..19` (Unix `nice`-style) integer range on every platform.
//! POSIX: real `getpriority(2)`/`setpriority(2)` with `PRIO_PROCESS`. Windows:
//! `GetPriorityClass`/`SetPriorityClass` over `OpenProcess`, bucketed through
//! the 6 `PRIORITY_*` classes exactly as libuv/Node do. `pid == 0` is the
//! calling process. Failures throw a real Error (`ESRCH`/`EACCES`/`EPERM`) via
//! the engine pending-error slot (members flagged `THROWS`). No faked values.
//!
//! `getPriority`/`setPriority` are each registered twice (arity 0/1 and 1/2)
//! so the optional leading `pid` resolves by argument count without a shim.

use super::words::throw_error;

// Node's PRIORITY_* nice values (also surfaced in `os.constants.priority`).
const PRIORITY_HIGHEST: i32 = -20;
const PRIORITY_HIGH: i32 = -14;
const PRIORITY_ABOVE_NORMAL: i32 = -7;
const PRIORITY_NORMAL: i32 = 0;
const PRIORITY_BELOW_NORMAL: i32 = 10;
const PRIORITY_LOW: i32 = 19;

/// `os.getPriority()` — priority of the calling process.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_GET_PRIORITY_SELF() -> i32 {
    get_priority(0)
}

/// `os.getPriority(pid)` — priority of process `pid` (`0` = self).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_GET_PRIORITY(pid: i32) -> i32 {
    get_priority(pid)
}

/// `os.setPriority(priority)` — set the calling process's priority.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_SET_PRIORITY_SELF(priority: i32) {
    set_priority(0, priority)
}

/// `os.setPriority(pid, priority)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_SET_PRIORITY(pid: i32, priority: i32) {
    set_priority(pid, priority)
}

/// Clamp an arbitrary requested value into Node's documented `-20..19` range.
fn clamp_nice(priority: i32) -> i32 {
    priority.clamp(PRIORITY_HIGHEST, PRIORITY_LOW)
}

// --- POSIX -------------------------------------------------------------------

/// Cross-Unix pointer to the thread-local `errno` (glibc `__errno_location`
/// vs BSD/macOS `__error`).
#[cfg(unix)]
fn errno_ptr() -> *mut i32 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        unsafe { libc::__errno_location() }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        unsafe { libc::__error() }
    }
}

#[cfg(unix)]
fn get_priority(pid: i32) -> i32 {
    // getpriority can legitimately return -1; disambiguate via errno.
    unsafe { *errno_ptr() = 0 };
    let prio = unsafe { libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t) };
    let errno = unsafe { *errno_ptr() };
    if prio == -1 && errno != 0 {
        throw_error("Error", &errno_message("getpriority", errno));
        return 0;
    }
    prio
}

#[cfg(unix)]
fn set_priority(pid: i32, priority: i32) {
    let nice = clamp_nice(priority);
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
    if rc != 0 {
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        throw_error("Error", &errno_message("setpriority", errno));
    }
}

#[cfg(unix)]
fn errno_message(syscall: &str, errno: i32) -> String {
    let code = match errno {
        libc::ESRCH => "ESRCH",
        libc::EPERM => "EPERM",
        libc::EACCES => "EACCES",
        libc::EINVAL => "EINVAL",
        _ => "EUNKNOWN",
    };
    format!("{code}: {syscall} failed (errno {errno})")
}

// --- Windows -----------------------------------------------------------------

#[cfg(windows)]
fn get_priority(pid: i32) -> i32 {
    match win::get_priority_class(pid) {
        Ok(class) => win::class_to_nice(class),
        Err(msg) => {
            throw_error("Error", &msg);
            0
        }
    }
}

#[cfg(windows)]
fn set_priority(pid: i32, priority: i32) {
    let class = win::nice_to_class(clamp_nice(priority));
    if let Err(msg) = win::set_priority_class(pid, class) {
        throw_error("Error", &msg);
    }
}

// --- other -------------------------------------------------------------------

#[cfg(not(any(unix, windows)))]
fn get_priority(_pid: i32) -> i32 {
    0
}

#[cfg(not(any(unix, windows)))]
fn set_priority(_pid: i32, _priority: i32) {}

#[cfg(windows)]
mod win {
    use super::{
        PRIORITY_ABOVE_NORMAL, PRIORITY_BELOW_NORMAL, PRIORITY_HIGH, PRIORITY_HIGHEST,
        PRIORITY_LOW, PRIORITY_NORMAL,
    };

    const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
    const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
    const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
    const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;
    const REALTIME_PRIORITY_CLASS: u32 = 0x0000_0100;

    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const PROCESS_SET_INFORMATION: u32 = 0x0200;

    type Handle = isize;

    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(h: Handle) -> i32;
        fn GetPriorityClass(h: Handle) -> u32;
        fn SetPriorityClass(h: Handle, class: u32) -> i32;
        fn GetLastError() -> u32;
    }

    /// Windows priority class → Node nice value (libuv's exact mapping).
    pub fn class_to_nice(class: u32) -> i32 {
        match class {
            REALTIME_PRIORITY_CLASS => PRIORITY_HIGHEST,
            HIGH_PRIORITY_CLASS => PRIORITY_HIGH,
            ABOVE_NORMAL_PRIORITY_CLASS => PRIORITY_ABOVE_NORMAL,
            BELOW_NORMAL_PRIORITY_CLASS => PRIORITY_BELOW_NORMAL,
            IDLE_PRIORITY_CLASS => PRIORITY_LOW,
            _ => PRIORITY_NORMAL,
        }
    }

    /// Node nice value → nearest Windows priority class (libuv's bucketing).
    pub fn nice_to_class(priority: i32) -> u32 {
        if priority < PRIORITY_HIGH {
            REALTIME_PRIORITY_CLASS
        } else if priority < PRIORITY_ABOVE_NORMAL {
            HIGH_PRIORITY_CLASS
        } else if priority < PRIORITY_NORMAL {
            ABOVE_NORMAL_PRIORITY_CLASS
        } else if priority < PRIORITY_BELOW_NORMAL {
            NORMAL_PRIORITY_CLASS
        } else if priority < PRIORITY_LOW {
            BELOW_NORMAL_PRIORITY_CLASS
        } else {
            IDLE_PRIORITY_CLASS
        }
    }

    /// Open a process handle for `pid` (`0` = current process pseudo-handle).
    fn open(pid: i32, access: u32) -> Result<(Handle, bool), String> {
        if pid == 0 {
            Ok((unsafe { GetCurrentProcess() }, false))
        } else {
            let h = unsafe { OpenProcess(access, 0, pid as u32) };
            if h == 0 {
                Err(open_error(pid))
            } else {
                Ok((h, true))
            }
        }
    }

    fn open_error(pid: i32) -> String {
        // ERROR_INVALID_PARAMETER (87) for a nonexistent pid → ESRCH;
        // ERROR_ACCESS_DENIED (5) → EPERM.
        match unsafe { GetLastError() } {
            5 => format!("EPERM: OpenProcess({pid}) access denied"),
            87 => format!("ESRCH: no process with pid {pid}"),
            e => format!("ESRCH: OpenProcess({pid}) failed (error {e})"),
        }
    }

    pub fn get_priority_class(pid: i32) -> Result<u32, String> {
        let (h, owned) = open(pid, PROCESS_QUERY_INFORMATION)?;
        let class = unsafe { GetPriorityClass(h) };
        if owned {
            unsafe { CloseHandle(h) };
        }
        if class == 0 {
            return Err(format!("EPERM: GetPriorityClass failed (error {})", unsafe {
                GetLastError()
            }));
        }
        Ok(class)
    }

    pub fn set_priority_class(pid: i32, class: u32) -> Result<(), String> {
        let (h, owned) = open(pid, PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION)?;
        let rc = unsafe { SetPriorityClass(h, class) };
        let err = if rc == 0 { unsafe { GetLastError() } } else { 0 };
        if owned {
            unsafe { CloseHandle(h) };
        }
        if rc == 0 {
            let code = if err == 5 { "EACCES" } else { "EPERM" };
            return Err(format!("{code}: SetPriorityClass failed (error {err})"));
        }
        Ok(())
    }
}
