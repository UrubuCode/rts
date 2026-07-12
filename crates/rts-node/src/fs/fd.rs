//! node:fs — the file-descriptor family (`openSync`/`readSync`/`writeSync`/
//! `closeSync`/`fstatSync`/`ftruncateSync`/`fsyncSync`). Open `File`s live in a
//! side table keyed by an integer fd (starting at 3, after stdio) — no engine
//! change; the fd that crosses to JS is a plain number. Real `std::fs` I/O.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::stats;
use super::words::{read, read_bytes, throw_io};
use rts_engine::heap::handles::{with_entry_mut, Entry};

fn table() -> &'static Mutex<HashMap<u64, File>> {
    static T: OnceLock<Mutex<HashMap<u64, File>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_FD: AtomicU64 = AtomicU64::new(3);

/// Map a Node `flags` string to `OpenOptions`.
fn options(flags: &str) -> OpenOptions {
    let mut o = OpenOptions::new();
    match flags {
        "r" => o.read(true),
        "r+" => o.read(true).write(true),
        "w" => o.write(true).create(true).truncate(true),
        "w+" => o.read(true).write(true).create(true).truncate(true),
        "a" => o.append(true).create(true),
        "a+" => o.read(true).append(true).create(true),
        "wx" => o.write(true).create_new(true),
        "ax" => o.append(true).create_new(true),
        _ => o.read(true),
    };
    o
}

/// Run `f` over the open `File` for `fd`, if any.
fn with_fd<R>(fd: u64, f: impl FnOnce(&mut File) -> R) -> Option<R> {
    table().lock().unwrap().get_mut(&fd).map(f)
}

/// `fs.openSync(path, flags)` → fd.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_OPEN(p: *const u8, l: i64, fp: *const u8, fl: i64) -> i64 {
    let path = read(p, l);
    let flags = read(fp, fl);
    let flags = if flags.is_empty() { "r".to_string() } else { flags };
    match options(&flags).open(&path) {
        Ok(file) => {
            let fd = NEXT_FD.fetch_add(1, Ordering::Relaxed);
            table().lock().unwrap().insert(fd, file);
            fd as i64
        }
        Err(e) => {
            throw_io(&e, "open", &path);
            -1
        }
    }
}

/// `fs.closeSync(fd)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CLOSE(fd: i64) {
    table().lock().unwrap().remove(&(fd as u64));
}

/// `fs.readSync(fd, buffer, offset, length, position)` → bytes read. `position`
/// < 0 means "read from the current file position" (no seek).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READ(fd: i64, buffer: u64, offset: i64, length: i64, position: i64) -> i64 {
    let len = length.max(0) as usize;
    let off = offset.max(0) as usize;
    let mut tmp = vec![0u8; len];
    let n = with_fd(fd as u64, |f| {
        if position >= 0 {
            let _ = f.seek(SeekFrom::Start(position as u64));
        }
        f.read(&mut tmp).unwrap_or(0)
    })
    .unwrap_or(0);
    // Copy the bytes into the JS buffer (Uint8Array-shaped Entry::Vec) at offset.
    with_entry_mut(buffer, |e| {
        if let Some(Entry::Vec(v)) = e {
            for i in 0..n {
                if off + i < v.len() {
                    v[off + i] = f64::from(tmp[i]).to_bits() as i64;
                }
            }
        }
    });
    n as i64
}

/// `fs.writeSync(fd, buffer, offset, length, position)` → bytes written.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WRITE(fd: i64, buffer: u64, offset: i64, length: i64, position: i64) -> i64 {
    let all = read_bytes(buffer);
    let off = offset.max(0) as usize;
    let end = (off + length.max(0) as usize).min(all.len());
    let slice = if off <= end { &all[off..end] } else { &[] };
    with_fd(fd as u64, |f| {
        if position >= 0 {
            let _ = f.seek(SeekFrom::Start(position as u64));
        }
        f.write(slice).unwrap_or(0)
    })
    .unwrap_or(0) as i64
}

/// `fs.fstatSync(fd)` → Stats.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FSTAT(fd: i64) -> u64 {
    match with_fd(fd as u64, |f| f.metadata()) {
        Some(Ok(m)) => stats::build(&m),
        _ => 0,
    }
}

/// `fs.ftruncateSync(fd, len)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FTRUNCATE(fd: i64, len: i64) {
    with_fd(fd as u64, |f| f.set_len(len.max(0) as u64).ok());
}

/// `fs.fsyncSync(fd)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FSYNC(fd: i64) {
    with_fd(fd as u64, |f| f.sync_all().ok());
}

/// `fs.fdatasyncSync(fd)` — flush file DATA (not necessarily metadata).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FDATASYNC(fd: i64) {
    with_fd(fd as u64, |f| f.sync_data().ok());
}

/// `fs.fchmodSync(fd, mode)` — set the open file's permission bits.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FCHMOD(fd: i64, mode: i64) {
    with_fd(fd as u64, |f| {
        let Ok(md) = f.metadata() else { return };
        let mut perms = md.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(mode as u32);
        }
        #[cfg(not(unix))]
        {
            perms.set_readonly(mode & 0o200 == 0);
        }
        f.set_permissions(perms).ok();
    });
}

/// `fs.futimesSync(fd, atime, mtime)` — set the open file's access/modify times
/// (seconds since the epoch).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FUTIMES(fd: i64, atime: f64, mtime: f64) {
    let to_time = |secs: f64| -> std::time::SystemTime {
        if secs >= 0.0 {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(secs)
        } else {
            std::time::UNIX_EPOCH - std::time::Duration::from_secs_f64(-secs)
        }
    };
    let times = std::fs::FileTimes::new().set_accessed(to_time(atime)).set_modified(to_time(mtime));
    with_fd(fd as u64, |f| f.set_times(times).ok());
}

/// `fs.fchownSync(fd, uid, gid)` — change the open file's ownership. Unix-only
/// effect (via `libc::fchown` on the real OS fd); a no-op on Windows, matching
/// Node's own platform behavior.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FCHOWN(fd: i64, uid: i64, gid: i64) {
    #[cfg(unix)]
    with_fd(fd as u64, |f| {
        use std::os::unix::io::AsRawFd;
        let r = unsafe { libc::fchown(f.as_raw_fd(), uid as libc::uid_t, gid as libc::gid_t) };
        if r != 0 {
            throw_io(&std::io::Error::last_os_error(), "fchown", "");
        }
    });
    #[cfg(not(unix))]
    let _ = (fd, uid, gid);
}
