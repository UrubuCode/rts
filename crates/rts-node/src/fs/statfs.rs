//! `statfsSync` — filesystem-level statistics (POSIX `statvfs`; Windows
//! `GetDiskFreeSpaceExW`), over one shared `StatFs` prototype with no
//! methods — the reference doc (§2.1) lists `StatFs` as properties only, so
//! [`entry::make_prototype`] is still used (per this task's instruction, and
//! for the same reason [`super::dirent`]/[`super::dir`] use it: one object
//! per class, remembered by name) but installed with an EMPTY method list.

use rts_core::entry;

struct Stat {
    kind: f64,
    bsize: f64,
    blocks: f64,
    bfree: f64,
    bavail: f64,
    files: f64,
    ffree: f64,
}

#[cfg(unix)]
fn query(path: &str) -> Option<Stat> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(std::ffi::OsStr::new(path).as_bytes()).ok()?;
    let mut buffer: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut buffer) } != 0 {
        return None;
    }
    Some(Stat {
        kind: buffer.f_fsid as f64,
        bsize: buffer.f_frsize as f64,
        blocks: buffer.f_blocks as f64,
        bfree: buffer.f_bfree as f64,
        bavail: buffer.f_bavail as f64,
        files: buffer.f_files as f64,
        ffree: buffer.f_ffree as f64,
    })
}

#[cfg(windows)]
fn query(path: &str) -> Option<Stat> {
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_bytes_available: *mut u64,
            total_bytes: *mut u64,
            total_free_bytes: *mut u64,
        ) -> i32;
    }
    // `GetDiskFreeSpaceExW` wants a directory/root, not necessarily an
    // existing file — the volume root of `path` is what it is asked about.
    let root = std::path::Path::new(path).ancestors().last().map(|p| p.to_string_lossy().into_owned())?;
    let mut wide: Vec<u16> = root.encode_utf16().collect();
    if !wide.last().is_some_and(|&c| c == b'\\' as u16) {
        wide.push(b'\\' as u16);
    }
    wide.push(0);
    let (mut free_available, mut total, mut total_free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_available, &mut total, &mut total_free) };
    if ok == 0 {
        return None;
    }
    // Windows has no notion of "block size" the way `statvfs` does, and no
    // filesystem-type numeric id at this API level — `bsize` is reported as
    // `1` (byte-granular) so `blocks`/`bfree`/`bavail` stay honest as BYTE
    // counts rather than a fabricated cluster size, and `type`/`files`/
    // `ffree` (inode-shaped concepts Windows filesystems do not expose this
    // way) are `0`.
    Some(Stat { kind: 0.0, bsize: 1.0, blocks: total as f64, bfree: total_free as f64, bavail: free_available as f64, files: 0.0, ffree: 0.0 })
}

#[cfg(not(any(unix, windows)))]
fn query(_path: &str) -> Option<Stat> {
    None
}

/// `fs.statfsSync(path, options?)`. `undefined` on failure. `options.bigint`
/// is read but not honored — this module has no `BigInt` constructor
/// reachable from a native, same gap [`super::stats`] states for `Date`.
pub(super) extern "C" fn statfs_sync(_e: u64, _this: u64, path: u64, _options: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = super::validate::path("path", path) else {
        return entry::undefined_value();
    };
    let Some(stat) = query(&path) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "StatFs", &[]);
        let instance = entry::make_instance(context, prototype);
        for (name, value) in [
            ("type", stat.kind),
            ("bsize", stat.bsize),
            ("blocks", stat.blocks),
            ("bfree", stat.bfree),
            ("bavail", stat.bavail),
            ("files", stat.files),
            ("ffree", stat.ffree),
        ] {
            let number = entry::make_number(value);
            entry::put_member(context, instance, name, number);
        }
        instance
    })
}
