//! Who the process is running as, and at what scheduling priority.
//!
//! The two are one file because they are the two members of `node:os` that ask
//! the OS about THIS process rather than about the machine, and both are the
//! same shape of call on both platforms.

/// `os.userInfo()`'s answer, before any encoding is applied.
pub(super) struct User {
    pub(super) username: String,
    /// `-1` on Windows, which has no uid — Node's own documented value there,
    /// not a stand-in invented here.
    pub(super) uid: f64,
    pub(super) gid: f64,
    /// `None` becomes `null`; Windows has no login shell.
    pub(super) shell: Option<String>,
    pub(super) homedir: String,
}

/// The current user, or `None` when the host will not say.
#[cfg(windows)]
pub(super) fn current() -> Option<User> {
    let mut buffer = [0u16; 256];
    let mut size = buffer.len() as u32;
    let ok = unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut size) };
    if ok == 0 || size == 0 {
        return None;
    }
    // The count `GetUserNameW` reports INCLUDES the terminating NUL, which is
    // why the slice stops one short: keeping it makes `username` compare
    // unequal to the same name from anywhere else, invisibly.
    let username = String::from_utf16_lossy(&buffer[..size as usize - 1]);
    Some(User { username, uid: -1.0, gid: -1.0, shell: None, homedir: super::home_directory() })
}

/// The current user, from the password database.
#[cfg(not(windows))]
pub(super) fn current() -> Option<User> {
    let uid = unsafe { libc::geteuid() };
    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut scratch = vec![0i8; 4096];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let status = unsafe {
        libc::getpwuid_r(uid, &mut passwd, scratch.as_mut_ptr(), scratch.len(), &mut found)
    };
    if status != 0 || found.is_null() {
        return None;
    }
    let text = |start: *const libc::c_char| match start.is_null() {
        true => String::new(),
        false => unsafe { std::ffi::CStr::from_ptr(start) }.to_string_lossy().into_owned(),
    };
    Some(User {
        username: text(passwd.pw_name),
        uid: f64::from(uid),
        gid: f64::from(unsafe { libc::getegid() }),
        // An empty `pw_shell` is a user with no login shell, which is `null`
        // rather than `""` — the same distinction Node draws.
        shell: Some(text(passwd.pw_shell)).filter(|held| !held.is_empty()),
        homedir: text(passwd.pw_dir),
    })
}

/// The object `os.userInfo()` answers, or `undefined` when the host will not
/// say who this is.
///
/// `encoding: 'buffer'` turns the three text fields into `Buffer`s of the SAME
/// bytes, which is why the branch is here rather than in a second reader: one
/// read of the password database, two spellings of its answer.
pub(super) fn value(context: &mut rts_core_rwk::entry::Context, as_buffer: bool) -> u64 {
    let Some(held) = current() else {
        return rts_core_rwk::entry::undefined_in(context);
    };
    let object = rts_core_rwk::entry::make_object(context);
    let text = |context: &mut rts_core_rwk::entry::Context, held: &str| match as_buffer {
        true => rts_core_rwk::entry::make_buffer(context, held.as_bytes()),
        false => rts_core_rwk::entry::make_string(context, held),
    };
    let username = text(context, &held.username);
    rts_core_rwk::entry::put_member(context, object, "username", username);
    let homedir = text(context, &held.homedir);
    rts_core_rwk::entry::put_member(context, object, "homedir", homedir);
    let shell = match &held.shell {
        Some(found) => text(context, found),
        None => rts_core_rwk::entry::null_in(context),
    };
    rts_core_rwk::entry::put_member(context, object, "shell", shell);
    for (name, number) in [("uid", held.uid), ("gid", held.gid)] {
        let value = rts_core_rwk::entry::make_number(number);
        rts_core_rwk::entry::put_member(context, object, name, value);
    }
    object
}

/// `os.getPriority(pid)` — `None` when the host refuses to answer.
///
/// This crate cannot throw across the native boundary (the same limitation
/// `node:fs` states), so an unknown pid or a permission failure is `None` here
/// and `undefined` to the program. That is a stated divergence from Node's
/// `ESRCH`/`EPERM` throw, and it is the honest one: an invented `0` would be a
/// priority the process does not have.
#[cfg(windows)]
pub(super) fn priority_of(pid: u32) -> Option<f64> {
    let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let class = unsafe { GetPriorityClass(handle) };
    close(handle, pid);
    (class != 0).then(|| f64::from(nice_of(class)))
}

/// `os.setPriority(pid, priority)` — whether it took.
#[cfg(windows)]
pub(super) fn set_priority(pid: u32, priority: f64) -> bool {
    let Some(handle) = open(pid, PROCESS_SET_INFORMATION) else { return false };
    let ok = unsafe { SetPriorityClass(handle, class_of(priority)) };
    close(handle, pid);
    ok != 0
}

/// `getpriority(2)`, with `errno` cleared first: `-1` is a legal priority AND
/// the failure return, so the two are only distinguishable through `errno`.
#[cfg(not(windows))]
pub(super) fn priority_of(pid: u32) -> Option<f64> {
    let slot = errno_slot();
    unsafe { *slot = 0 };
    let held = unsafe { libc::getpriority(libc::PRIO_PROCESS, pid) };
    match unsafe { *slot } {
        0 => Some(f64::from(held)),
        _ => None,
    }
}

/// Where this libc keeps `errno`.
///
/// Not `std::io::Error::last_os_error`, which can only READ it: the value has
/// to be cleared before the call for the read afterwards to mean anything.
#[cfg(target_os = "linux")]
fn errno_slot() -> *mut i32 {
    unsafe { libc::__errno_location() }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn errno_slot() -> *mut i32 {
    unsafe { libc::__error() }
}

/// `setpriority(2)`.
#[cfg(not(windows))]
pub(super) fn set_priority(pid: u32, priority: f64) -> bool {
    let clamped = priority.clamp(-20.0, 19.0) as i32;
    unsafe { libc::setpriority(libc::PRIO_PROCESS, pid, clamped) == 0 }
}

/// A process handle, or the pseudo-handle for this process.
///
/// `pid` `0` means "the calling process" for both members, which is the
/// `nice(2)` convention Node keeps — and the pseudo-handle needs no close,
/// which is what [`close`] checks the pid for.
#[cfg(windows)]
fn open(pid: u32, access: u32) -> Option<isize> {
    if pid == 0 {
        return Some(unsafe { GetCurrentProcess() });
    }
    let handle = unsafe { OpenProcess(access, 0, pid) };
    (handle != 0).then_some(handle)
}

#[cfg(windows)]
fn close(handle: isize, pid: u32) {
    if pid != 0 {
        unsafe { CloseHandle(handle) };
    }
}

/// A Windows priority class as the `nice`-style integer Node reports.
///
/// Node exposes one `-20..19` range on every platform, so Windows' six discrete
/// classes are mapped onto the six `PRIORITY_*` values — the same table
/// `os.constants.priority` publishes, which is why it is not written twice.
#[cfg(windows)]
fn nice_of(class: u32) -> i32 {
    match class {
        REALTIME_PRIORITY_CLASS => -20,
        HIGH_PRIORITY_CLASS => -14,
        ABOVE_NORMAL_PRIORITY_CLASS => -7,
        BELOW_NORMAL_PRIORITY_CLASS => 10,
        IDLE_PRIORITY_CLASS => 19,
        _ => 0,
    }
}

/// The inverse: a requested integer bucketed into the nearest class.
///
/// Bucketed by RANGE rather than by nearest value, matching libuv — a request
/// for `-15` is `REALTIME` there, and rounding it to `HIGH` instead would make
/// `setPriority(x); getPriority()` disagree with Node for half the range.
#[cfg(windows)]
fn class_of(priority: f64) -> u32 {
    let held = priority.clamp(-20.0, 19.0);
    if held < -14.0 {
        REALTIME_PRIORITY_CLASS
    } else if held < -7.0 {
        HIGH_PRIORITY_CLASS
    } else if held < 0.0 {
        ABOVE_NORMAL_PRIORITY_CLASS
    } else if held < 10.0 {
        NORMAL_PRIORITY_CLASS
    } else if held < 19.0 {
        BELOW_NORMAL_PRIORITY_CLASS
    } else {
        IDLE_PRIORITY_CLASS
    }
}

#[cfg(windows)]
const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
#[cfg(windows)]
const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
#[cfg(windows)]
const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;
#[cfg(windows)]
const REALTIME_PRIORITY_CLASS: u32 = 0x0000_0100;
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
#[cfg(windows)]
const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
#[cfg(windows)]
const PROCESS_SET_INFORMATION: u32 = 0x0000_0200;
#[cfg(windows)]
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn GetPriorityClass(handle: isize) -> u32;
    fn SetPriorityClass(handle: isize, class: u32) -> i32;
}

#[cfg(windows)]
#[link(name = "advapi32")]
unsafe extern "system" {
    fn GetUserNameW(buffer: *mut u16, size: *mut u32) -> i32;
}
