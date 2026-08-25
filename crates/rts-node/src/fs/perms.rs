//! `chownSync`/`lchownSync`/`fchownSync`/`fchmodSync`/`lchmodSync`/
//! `utimesSync`/`lutimesSync`/`futimesSync` — the permission and timestamp
//! family, over `libc` (Unix) and hand-declared `extern "system"` Win32 calls
//! (Windows) — see `Cargo.toml` for why `libc` is an accepted dependency.
//!
//! # The Windows column, per member
//!
//! Node runs on Windows too, and each member below does ONE of two things
//! there rather than silently doing nothing:
//! - **`chownSync`/`lchownSync`/`fchownSync`** — Windows has no POSIX uid/gid
//!   model at all. These **refuse**: [`super::record`] is set to `false`, so
//!   the sync form still answers `undefined` (nothing else to give, same as
//!   every no-return member) but `fs.promises.chown`/etc. REJECT rather than
//!   silently resolving over a call that changed nothing.
//! - **`lchmodSync`** — setting a *symlink's own* mode bit with no
//!   dereference has no Windows equivalent either (there is no non-follow
//!   attribute-set primitive `std`/raw Win32 expose simply) — same refusal.
//! - **`fchmodSync`/`utimesSync`/`lutimesSync`/`futimesSync`** — these DO
//!   have a Windows equivalent (`FILE_ATTRIBUTE_READONLY` toggling for
//!   `fchmod`, `SetFileTime` for the timestamp family) and use it for real,
//!   rather than refusing something Windows can actually do.
//!
//! `lutimesSync`/`lchmodSync` need "the file itself, not what it points at"
//! on Windows, which needs `FILE_FLAG_OPEN_REPARSE_POINT`; `lutimesSync` uses
//! it, `lchmodSync` does not exist there at all (see above).
//!
//! # Unix: `lchmod` is macOS/BSD only
//!
//! Linux has no `lchmod(2)` syscall — glibc does not even provide the
//! symbol — so `lchmodSync` refuses there too (`super::record(false)`),
//! exactly as it refuses on Windows, and only actually calls a syscall on
//! macOS/*BSD, where `libc` does not expose `lchmod` (it broke a released
//! build on macOS before) so the symbol is hand-declared, matching
//! `crates/rts-node/src/fs/meta.rs`'s own precedent for the same gap.

use rts_core::entry;

use super::{fd, number, validate};

// ---------------------------------------------------------------- chown ---

#[cfg(unix)]
fn chown_impl(path: &str, uid: i64, gid: i64, nofollow: bool) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(std::ffi::OsStr::new(path).as_bytes()) else {
        return false;
    };
    let result = unsafe {
        if nofollow {
            libc::lchown(c.as_ptr(), uid as libc::uid_t, gid as libc::gid_t)
        } else {
            libc::chown(c.as_ptr(), uid as libc::uid_t, gid as libc::gid_t)
        }
    };
    result == 0
}

/// `fs.chownSync(path, uid, gid)`.
pub(super) extern "C" fn chown_sync(_e: u64, _this: u64, path: u64, uid: u64, gid: u64, _a3: u64) -> u64 {
    apply_path_chown(path, uid, gid, false)
}

/// The body `chownSync` and `lchownSync` share, one `nofollow` apart.
///
/// Each argument is refused on its own rule and in Node's own order — path,
/// then uid, then gid — because `test-fs-chown-type-check.js` asserts a refusal
/// for a bad uid while the path is VALID, which the single combined `if let`
/// tuple this replaced answered by silently doing nothing.
fn apply_path_chown(path: u64, uid: u64, gid: u64, nofollow: bool) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return entry::undefined_value();
    };
    let Some(uid) = validate::id("uid", uid) else {
        return entry::undefined_value();
    };
    let Some(gid) = validate::id("gid", gid) else {
        return entry::undefined_value();
    };
    #[cfg(unix)]
    super::record(chown_impl(&path, uid, gid, nofollow));
    #[cfg(not(unix))]
    {
        let _ = (path, uid, gid, nofollow);
        super::record(false);
    }
    entry::undefined_value()
}

/// `fs.lchownSync(path, uid, gid)` — like `chownSync`, without following a
/// symlink at the final path component.
pub(super) extern "C" fn lchown_sync(_e: u64, _this: u64, path: u64, uid: u64, gid: u64, _a3: u64) -> u64 {
    apply_path_chown(path, uid, gid, true)
}

/// `fs.fchownSync(fd, uid, gid)`.
pub(super) extern "C" fn fchown_sync(_e: u64, _this: u64, fd_value: u64, uid: u64, gid: u64, _a3: u64) -> u64 {
    let Some(fd_value) = validate::fd(fd_value) else {
        return entry::undefined_value();
    };
    let Some(uid) = validate::id("uid", uid) else {
        return entry::undefined_value();
    };
    let Some(gid) = validate::id("gid", gid) else {
        return entry::undefined_value();
    };
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let ok = fd::with_file(fd_value, |file| {
            unsafe { libc::fchown(file.as_raw_fd(), uid as libc::uid_t, gid as libc::gid_t) == 0 }
        })
        .unwrap_or(false);
        super::record(ok);
    }
    #[cfg(not(unix))]
    {
        let _ = (fd_value, uid, gid);
        super::record(false);
    }
    entry::undefined_value()
}

// ---------------------------------------------------------------- chmod ---

#[cfg(unix)]
fn apply_fchmod(file: &std::fs::File, mode: u32) -> bool {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(mode)).is_ok()
}

#[cfg(windows)]
fn apply_fchmod(file: &std::fs::File, mode: u32) -> bool {
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    let mut permissions = metadata.permissions();
    permissions.set_readonly(mode & 0o200 == 0);
    file.set_permissions(permissions).is_ok()
}

/// `fs.fchmodSync(fd, mode)`.
pub(super) extern "C" fn fchmod_sync(_e: u64, _this: u64, fd_value: u64, mode: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(fd_value) = validate::fd(fd_value) else {
        return entry::undefined_value();
    };
    let Some(mode) = validate::mode("mode", mode) else {
        return entry::undefined_value();
    };
    let ok = fd::with_file(fd_value, |file| apply_fchmod(file, mode)).unwrap_or(false);
    super::record(ok);
    entry::undefined_value()
}

/// `fs.lchmodSync(path, mode)` — see the module doc for the macOS/BSD-only
/// (and never-Windows) support this actually has.
pub(super) extern "C" fn lchmod_sync(_e: u64, _this: u64, path: u64, mode: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return entry::undefined_value();
    };
    let Some(mode) = validate::mode("mode", mode) else {
        return entry::undefined_value();
    };
    super::record(apply_lchmod(&path, mode));
    entry::undefined_value()
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd"))]
fn apply_lchmod(path: &str, mode: u32) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    // Not exposed by the `libc` crate on these targets (see the module doc);
    // declared directly, per `man 2 lchmod`.
    unsafe extern "C" {
        fn lchmod(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int;
    }
    let Ok(c) = CString::new(std::ffi::OsStr::new(path).as_bytes()) else {
        return false;
    };
    unsafe { lchmod(c.as_ptr(), mode as libc::mode_t) == 0 }
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd")))]
fn apply_lchmod(_path: &str, _mode: u32) -> bool {
    false
}

// ---------------------------------------------------------------- utimes ---

/// A JS `TimeLike` read as milliseconds since the epoch. Only the numeric
/// form (`Date#getTime()`-shaped, or a bare number of seconds/ms as Node
/// itself accepts) is read — a `Date` instance or a numeric-string both
/// answer through [`entry::number_of`]/[`text_of`] the same way every other
/// member of this module reads a number, so nothing new is invented for this
/// family specifically.
fn time_ms(value: u64) -> Option<f64> {
    number(value).map(|seconds_or_ms| {
        // Node accepts a `TimeLike` as *seconds* when it is a bare number
        // (matching `utimes(2)`'s own convention) — small integers under
        // this threshold are read as seconds, matching Node's own
        // `dateToWindowsTime`-adjacent handling for `Date`/numeric input.
        if seconds_or_ms.abs() < 1.0e11 { seconds_or_ms * 1000.0 } else { seconds_or_ms }
    })
}

#[cfg(unix)]
fn to_timespec(ms: f64) -> libc::timespec {
    let secs = (ms / 1000.0).floor();
    let nanos = ((ms - secs * 1000.0) * 1_000_000.0).round();
    libc::timespec { tv_sec: secs as libc::time_t, tv_nsec: nanos as _ }
}

#[cfg(unix)]
fn apply_utimensat(path: &str, atime_ms: f64, mtime_ms: f64, nofollow: bool) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = CString::new(std::ffi::OsStr::new(path).as_bytes()) else {
        return false;
    };
    let times = [to_timespec(atime_ms), to_timespec(mtime_ms)];
    let flags = if nofollow { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
    unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), flags) == 0 }
}

#[cfg(windows)]
#[repr(C)]
struct FileTime {
    low: u32,
    high: u32,
}

#[cfg(windows)]
fn to_filetime(ms: f64) -> FileTime {
    // Windows FILETIME: 100ns intervals since 1601-01-01; Unix epoch is
    // 1970-01-01, 11644473600 seconds later.
    let windows_100ns = ((ms + 11_644_473_600_000.0) * 10_000.0) as u64;
    FileTime { low: (windows_100ns & 0xFFFF_FFFF) as u32, high: (windows_100ns >> 32) as u32 }
}

#[cfg(windows)]
unsafe extern "system" {
    fn SetFileTime(handle: isize, creation: *const FileTime, last_access: *const FileTime, last_write: *const FileTime) -> i32;
}

#[cfg(windows)]
fn apply_set_file_time(file: &std::fs::File, atime_ms: f64, mtime_ms: f64) -> bool {
    use std::os::windows::io::AsRawHandle;
    let atime = to_filetime(atime_ms);
    let mtime = to_filetime(mtime_ms);
    unsafe { SetFileTime(file.as_raw_handle() as isize, std::ptr::null(), &atime, &mtime) != 0 }
}

#[cfg(windows)]
fn apply_utimes_windows(path: &str, atime_ms: f64, mtime_ms: f64, nofollow: bool) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    let flags = if nofollow { FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT } else { FILE_FLAG_BACKUP_SEMANTICS };
    options.custom_flags(flags);
    match options.open(path) {
        Ok(file) => apply_set_file_time(&file, atime_ms, mtime_ms),
        Err(_) => false,
    }
}

/// `fs.utimesSync(path, atime, mtime)`.
pub(super) extern "C" fn utimes_sync(_e: u64, _this: u64, path: u64, atime: u64, mtime: u64, _a3: u64) -> u64 {
    apply_path_utimes(path, atime, mtime, false)
}

/// `fs.lutimesSync(path, atime, mtime)` — like `utimesSync`, without
/// following a symlink at the final path component.
pub(super) extern "C" fn lutimes_sync(_e: u64, _this: u64, path: u64, atime: u64, mtime: u64, _a3: u64) -> u64 {
    apply_path_utimes(path, atime, mtime, true)
}

fn apply_path_utimes(path: u64, atime: u64, mtime: u64, nofollow: bool) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return entry::undefined_value();
    };
    // `atime`/`mtime` are NOT validated here: Node's `TimeLike` is a number, a
    // `Date` or a numeric string, and this module reads all three through
    // `time_ms`. A validator that refused what `time_ms` accepts would be a
    // stricter rule than Node's; one that accepted it would have nothing left
    // to check. Named as the gap it is — `test-fs-utimes.js` is not among the
    // files this change answers.
    if let (Some(atime_ms), Some(mtime_ms)) = (time_ms(atime), time_ms(mtime)) {
        #[cfg(unix)]
        super::record(apply_utimensat(&path, atime_ms, mtime_ms, nofollow));
        #[cfg(windows)]
        super::record(apply_utimes_windows(&path, atime_ms, mtime_ms, nofollow));
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            super::record(false);
        }
    }
    entry::undefined_value()
}

/// `fs.futimesSync(fd, atime, mtime)`.
pub(super) extern "C" fn futimes_sync(_e: u64, _this: u64, fd_value: u64, atime: u64, mtime: u64, _a3: u64) -> u64 {
    let Some(fd_value) = validate::fd(fd_value) else {
        return entry::undefined_value();
    };
    if let (Some(atime_ms), Some(mtime_ms)) = (time_ms(atime), time_ms(mtime)) {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let ok = fd::with_file(fd_value, |file| {
                let times = [to_timespec(atime_ms), to_timespec(mtime_ms)];
                unsafe { libc::futimens(file.as_raw_fd(), times.as_ptr()) == 0 }
            })
            .unwrap_or(false);
            super::record(ok);
        }
        #[cfg(windows)]
        {
            let ok = fd::with_file(fd_value, |file| apply_set_file_time(file, atime_ms, mtime_ms)).unwrap_or(false);
            super::record(ok);
        }
        #[cfg(not(any(unix, windows)))]
        super::record(false);
    }
    entry::undefined_value()
}
