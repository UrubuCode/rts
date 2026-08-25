//! `accessSync`/`readlinkSync`/`realpathSync`/`symlinkSync`/`linkSync`/
//! `chmodSync`/`truncateSync` — path-identity and permission-bit members that
//! are neither whole-file I/O nor directory-tree walks.

use super::{string, text, validate};

/// `fs.accessSync(path, mode?)`.
///
/// Node throws (`ENOENT`) on a failed check and answers `undefined` on
/// success; this now raises a catchable error for the failure case too
/// (`entry::throw_type_error`, per rule 8 of `rts-core`'s README — the
/// class raised is `TypeError` rather than Node's own `Error` with `code:
/// 'ENOENT'`, since `throw_type_error` is the only raise this crate can
/// reach publicly). `mode` (F_OK/R_OK/W_OK/X_OK) is read but only existence
/// is actually checked: distinguishing execute/write/read bits per-platform
/// with no way to carry the distinction into the message is not worth the
/// native call.
pub(super) extern "C" fn access_sync(_e: u64, _this: u64, path: u64, _mode: u64, _a2: u64, _a3: u64) -> u64 {
    // The refusal comes FIRST and returns: a bad argument is
    // `ERR_INVALID_ARG_TYPE`/`ERR_INVALID_ARG_VALUE`, never the `ENOENT`
    // below, and raising both would leave two throws pending for one call.
    let Some(path) = validate::path("path", path) else {
        return rts_core::entry::undefined_value();
    };
    let exists = std::path::Path::new(&path).exists();
    if !exists {
        rts_core::entry::throw_type_error("ENOENT: no such file or directory, access");
    }
    rts_core::entry::undefined_value()
}

/// `fs.readlinkSync(path)` — the text a symlink points at.
///
/// `undefined` on failure (not a symlink, missing path).
pub(super) extern "C" fn readlink_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return rts_core::entry::undefined_value();
    };
    match std::fs::read_link(&path) {
        Ok(target) => string(&target.to_string_lossy()),
        Err(_) => rts_core::entry::undefined_value(),
    }
}

/// `fs.realpathSync(path)` — the canonical, symlink-resolved path.
///
/// `undefined` on failure (`ENOENT`, a symlink cycle).
pub(super) extern "C" fn realpath_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return rts_core::entry::undefined_value();
    };
    match std::fs::canonicalize(&path) {
        Ok(resolved) => {
            let text = resolved.to_string_lossy();
            // `canonicalize` on Windows returns a `\\?\`-prefixed verbatim
            // path; Node's `realpathSync` does not, so the prefix is
            // stripped to match what a program actually gets from Node.
            let stripped = text.strip_prefix(r"\\?\").unwrap_or(&text);
            string(stripped)
        }
        Err(_) => rts_core::entry::undefined_value(),
    }
}

/// `fs.symlinkSync(target, path, type?)`.
///
/// `type` matters only on Windows (`'dir'` vs `'file'`/`'junction'`); when
/// omitted, Node auto-detects `'dir'` from whether `target` exists and is a
/// directory, which is reproduced here rather than defaulting to `'file'`
/// unconditionally.
pub(super) extern "C" fn symlink_sync(_e: u64, _this: u64, target: u64, path: u64, kind: u64, _a3: u64) -> u64 {
    let Some(target) = validate::path("target", target) else {
        return rts_core::entry::undefined_value();
    };
    let Some(path) = validate::path("path", path) else {
        return rts_core::entry::undefined_value();
    };
    // `type` stays read as loose text: Node's own check is a whitelist of three
    // strings and `undefined`, and `create_symlink` already treats anything
    // else as "detect from the target", so refusing it here would be a
    // stricter rule than Node's rather than the same one.
    super::record(create_symlink(&target, &path, text(kind)).is_ok());
    rts_core::entry::undefined_value()
}

#[cfg(unix)]
fn create_symlink(target: &str, path: &str, _kind: Option<String>) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}

#[cfg(windows)]
fn create_symlink(target: &str, path: &str, kind: Option<String>) -> std::io::Result<()> {
    let as_dir = match kind.as_deref() {
        Some("dir") | Some("junction") => true,
        Some("file") => false,
        _ => std::path::Path::new(target).is_dir(),
    };
    if as_dir {
        std::os::windows::fs::symlink_dir(target, path)
    } else {
        std::os::windows::fs::symlink_file(target, path)
    }
}

/// `fs.linkSync(existingPath, newPath)` — a hard link.
pub(super) extern "C" fn link_sync(_e: u64, _this: u64, existing: u64, new: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(existing) = validate::path("existingPath", existing) else {
        return rts_core::entry::undefined_value();
    };
    let Some(new) = validate::path("newPath", new) else {
        return rts_core::entry::undefined_value();
    };
    super::record(std::fs::hard_link(existing, new).is_ok());
    rts_core::entry::undefined_value()
}

/// `fs.chmodSync(path, mode)`.
///
/// POSIX honors the full bit pattern; on Windows only the read-only
/// attribute is toggled (`mode & 0o200` clear → read-only), matching the
/// reference doc's stated Windows divergence.
pub(super) extern "C" fn chmod_sync(_e: u64, _this: u64, path: u64, mode: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return rts_core::entry::undefined_value();
    };
    let Some(mode) = validate::mode("mode", mode) else {
        return rts_core::entry::undefined_value();
    };
    super::record(apply_chmod(&path, mode).is_ok());
    rts_core::entry::undefined_value()
}

#[cfg(unix)]
fn apply_chmod(path: &str, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(windows)]
fn apply_chmod(path: &str, mode: u32) -> std::io::Result<()> {
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_readonly(mode & 0o200 == 0);
    std::fs::set_permissions(path, perms)
}

/// `fs.truncateSync(path, len?)` — default `len = 0`. Opens the path
/// internally (not fd-based — see `ftruncateSync` in [`super::fd`] for the
/// fd form).
pub(super) extern "C" fn truncate_sync(_e: u64, _this: u64, path: u64, len: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = validate::path("path", path) else {
        return rts_core::entry::undefined_value();
    };
    let Some(len) = validate::length("len", len) else {
        return rts_core::entry::undefined_value();
    };
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        super::record(file.set_len(len).is_ok());
    }
    rts_core::entry::undefined_value()
}
