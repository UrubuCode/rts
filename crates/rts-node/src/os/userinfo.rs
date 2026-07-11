//! node:os — `os.userInfo()`.
//!
//! Returns `{ username, uid, gid, shell, homedir }`. POSIX: real
//! `geteuid`/`getegid` + `getpwuid_r(3)` for username/homedir/shell (falls back
//! to `$USER`/`$HOME`/`$SHELL` only if the passwd lookup fails — never a
//! fabricated value). Windows: `uid`/`gid` are `-1` and `shell` is `null`
//! (matching Node; no POSIX identity concept), `username` from `GetUserNameW`.
//!
//! The `{ encoding: 'buffer' }` option variant (string fields as `Buffer`) is
//! deferred: it needs an options-object shim layer rts-node doesn't ship yet —
//! deferred honestly, not faked. The default string form is fully real.

use super::words::{null_w, num_word, object, str_word};

struct Identity {
    username: String,
    uid: f64,
    gid: f64,
    /// `Some(path)` on POSIX, `None` (→ JS `null`) on Windows.
    shell: Option<String>,
    homedir: String,
}

/// `os.userInfo()` — real current-user identity object.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_USER_INFO() -> u64 {
    let id = current_identity();
    let shell_word = match &id.shell {
        Some(s) => str_word(s),
        None => null_w(),
    };
    let keys: &[&str] = &["uid", "gid", "username", "homedir", "shell"];
    let values: [i64; 5] = [
        num_word(id.uid),
        num_word(id.gid),
        str_word(&id.username),
        str_word(&id.homedir),
        shell_word,
    ];
    object(keys, &values)
}

#[cfg(unix)]
fn current_identity() -> Identity {
    let uid = unsafe { libc::geteuid() };
    let gid = unsafe { libc::getegid() };
    if let Some(pw) = passwd_for_uid(uid) {
        return Identity {
            username: pw.name,
            uid: uid as f64,
            gid: gid as f64,
            shell: Some(pw.shell),
            homedir: pw.dir,
        };
    }
    // passwd lookup failed — fall back to the environment (still real values).
    Identity {
        username: std::env::var("USER").unwrap_or_default(),
        uid: uid as f64,
        gid: gid as f64,
        shell: Some(std::env::var("SHELL").unwrap_or_default()),
        homedir: std::env::var("HOME").unwrap_or_default(),
    }
}

#[cfg(windows)]
fn current_identity() -> Identity {
    Identity {
        username: win::user_name()
            .unwrap_or_else(|| std::env::var("USERNAME").unwrap_or_default()),
        uid: -1.0,
        gid: -1.0,
        shell: None,
        homedir: std::env::var("USERPROFILE").unwrap_or_default(),
    }
}

#[cfg(not(any(unix, windows)))]
fn current_identity() -> Identity {
    Identity {
        username: std::env::var("USER").unwrap_or_default(),
        uid: -1.0,
        gid: -1.0,
        shell: None,
        homedir: std::env::var("HOME").unwrap_or_default(),
    }
}

/// `os.homedir()`'s POSIX passwd-database fallback (used when `$HOME` is unset).
#[cfg(unix)]
pub fn passwd_homedir() -> Option<String> {
    let uid = unsafe { libc::geteuid() };
    passwd_for_uid(uid).map(|pw| pw.dir)
}

#[cfg(unix)]
struct PasswdEntry {
    name: String,
    dir: String,
    shell: String,
}

/// `getpwuid_r(3)` for `uid`, decoding name/home/shell. `None` on lookup
/// failure or a user with no matching passwd entry.
#[cfg(unix)]
fn passwd_for_uid(uid: libc::uid_t) -> Option<PasswdEntry> {
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    // Start with the suggested buffer size, growing on ERANGE.
    let mut bufsize = match unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) } {
        n if n > 0 => n as usize,
        _ => 1024,
    };
    loop {
        let mut buf = vec![0i8; bufsize];
        // SAFETY: pwd/result/buf are valid; getpwuid_r fills them or returns errno.
        let rc = unsafe {
            libc::getpwuid_r(
                uid,
                &mut pwd,
                buf.as_mut_ptr() as *mut libc::c_char,
                bufsize,
                &mut result,
            )
        };
        if rc == libc::ERANGE && bufsize < 1 << 20 {
            bufsize *= 2;
            continue;
        }
        if rc != 0 || result.is_null() {
            return None;
        }
        return Some(PasswdEntry {
            name: cstr(pwd.pw_name),
            dir: cstr(pwd.pw_dir),
            shell: cstr(pwd.pw_shell),
        });
    }
}

/// Decode a borrowed C string pointer (a passwd field) to `String`, `""` if null.
#[cfg(unix)]
fn cstr(ptr: *const libc::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: passwd fields are NUL-terminated C strings valid for this call.
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
mod win {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn GetUserNameW(buffer: *mut u16, size: *mut u32) -> i32;
    }

    /// The current process user's login name via `GetUserNameW`.
    pub fn user_name() -> Option<String> {
        let mut size: u32 = 0;
        // First call sizes the buffer (fails with ERROR_INSUFFICIENT_BUFFER).
        unsafe { GetUserNameW(std::ptr::null_mut(), &mut size) };
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u16; size as usize];
        let rc = unsafe { GetUserNameW(buf.as_mut_ptr(), &mut size) };
        if rc == 0 {
            return None;
        }
        // size includes the trailing NUL — drop it.
        let end = (size as usize).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}
