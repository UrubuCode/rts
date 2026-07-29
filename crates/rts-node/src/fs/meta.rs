//! node:fs — symlink + ownership/permission metadata ops that std does not
//! expose portably: `symlinkSync`, `chownSync`, `lchownSync`, `lchmodSync`.
//! POSIX ownership goes through `libc` (`chown`/`lchown`/`lchmod`); on Windows
//! the ownership ops are no-ops (Node reports success without effect on Windows,
//! matching its own behavior). Real syscalls, no fabricated results.
//!
//! Authored with `#[rtse::function]` (symbol/signature/ts/doc all derived from
//! the Rust declaration); the macro fixes `MemberFlags::NONE`, so `fs/mod.rs`
//! patches `THROWS` on at registration (`throws(...)`), matching `serde_ns`.

use super::words::throw_io;

/// `fs.symlinkSync(target, path)` — create a symbolic link at `path` pointing to
/// `target`. On Windows the link kind (file vs directory) is chosen from the
/// target's type (Node's `type` argument default is `'file'`, upgraded to
/// `'dir'`/`'junction'` when the target is a directory).
#[rtse::function(module = "node:fs", value = "symlinkSync")]
fn symlink_sync(target: &str, path: &str) {
    #[cfg(unix)]
    let r = std::os::unix::fs::symlink(target, path);
    #[cfg(windows)]
    let r = if std::fs::metadata(target).map(|m| m.is_dir()).unwrap_or(false) {
        std::os::windows::fs::symlink_dir(target, path)
    } else {
        std::os::windows::fs::symlink_file(target, path)
    };
    #[cfg(not(any(unix, windows)))]
    let r: std::io::Result<()> = Ok(());
    if let Err(e) = r {
        throw_io(&e, "symlink", path);
    }
}

/// Change ownership of `path`. `nofollow` selects `lchown` (does not dereference
/// a symlink) over `chown`. Unix-only effect; a non-zero return raises the errno.
#[cfg(unix)]
fn chown_impl(path: &str, uid: i64, gid: i64, nofollow: bool, op: &str) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = match CString::new(std::ffi::OsStr::new(path).as_bytes()) {
        Ok(c) => c,
        Err(_) => return,
    };
    let r = unsafe {
        if nofollow {
            libc::lchown(c.as_ptr(), uid as libc::uid_t, gid as libc::gid_t)
        } else {
            libc::chown(c.as_ptr(), uid as libc::uid_t, gid as libc::gid_t)
        }
    };
    if r != 0 {
        throw_io(&std::io::Error::last_os_error(), op, path);
    }
}

/// Windows has no POSIX ownership model; Node's `chown`/`lchown` are effectively
/// no-ops there (they resolve successfully without changing anything).
#[cfg(not(unix))]
fn chown_impl(_path: &str, _uid: i64, _gid: i64, _nofollow: bool, _op: &str) {}

/// `fs.chownSync(path, uid, gid)`.
#[rtse::function(module = "node:fs", value = "chownSync")]
fn chown_sync(path: &str, uid: i64, gid: i64) {
    chown_impl(path, uid, gid, false, "chown");
}

/// `fs.lchownSync(path, uid, gid)` — like `chownSync` but does not follow a
/// symbolic link at `path`.
#[rtse::function(module = "node:fs", value = "lchownSync")]
fn lchown_sync(path: &str, uid: i64, gid: i64) {
    chown_impl(path, uid, gid, true, "lchown");
}

/// `fs.lchmodSync(path, mode)` — change a symlink's own permission bits. Only
/// macOS/BSD implement `lchmod`; Node exposes it only there, and on Linux/Windows
/// this is a no-op (the syscall does not exist), matching Node's own platform
/// coverage.
#[rtse::function(module = "node:fs", value = "lchmodSync")]
fn lchmod_sync(path: &str, mode: i64) {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd"))]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        // The `libc` crate does not expose `lchmod` on these targets (it broke the
        // macOS release build with E0425), so declare the extern directly — it is
        // a standard BSD/macOS libc symbol the linker resolves against the C
        // runtime. Signature per `man 2 lchmod`.
        unsafe extern "C" {
            fn lchmod(path: *const libc::c_char, mode: libc::mode_t) -> libc::c_int;
        }
        if let Ok(c) = CString::new(std::ffi::OsStr::new(path).as_bytes()) {
            let r = unsafe { lchmod(c.as_ptr(), mode as libc::mode_t) };
            if r != 0 {
                throw_io(&std::io::Error::last_os_error(), "lchmod", path);
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd")))]
    {
        let _ = (path, mode);
    }
}
