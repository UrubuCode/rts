//! node:fs — `statfsSync(path)`: filesystem statistics as a PLAIN object
//! (`{ type, bsize, blocks, bfree, bavail, files, ffree }`) — field access only,
//! no methods, so no object-backed-class dispatch is involved. Real syscalls
//! (POSIX `statvfs`, Win32 `GetDiskFreeSpaceW`).

use rts_engine::heap::shapes::alloc_shaped_object;

use super::words::{read, throw_io};

struct FsStat {
    fstype: f64,
    bsize: f64,
    blocks: f64,
    bfree: f64,
    bavail: f64,
    files: f64,
    ffree: f64,
}

#[cfg(unix)]
fn query(path: &str) -> std::io::Result<FsStat> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(std::ffi::OsStr::new(path).as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let mut st: libc::statvfs = unsafe { core::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(FsStat {
        fstype: st.f_fsid as f64,
        bsize: st.f_frsize as f64,
        blocks: st.f_blocks as f64,
        bfree: st.f_bfree as f64,
        bavail: st.f_bavail as f64,
        files: st.f_files as f64,
        ffree: st.f_ffree as f64,
    })
}

#[cfg(windows)]
fn query(path: &str) -> std::io::Result<FsStat> {
    unsafe extern "system" {
        fn GetDiskFreeSpaceW(
            root: *const u16,
            sectors_per_cluster: *mut u32,
            bytes_per_sector: *mut u32,
            free_clusters: *mut u32,
            total_clusters: *mut u32,
        ) -> i32;
    }
    // Resolve to the volume root of `path` (GetDiskFreeSpaceW wants a root/dir).
    let root = std::path::Path::new(path)
        .ancestors()
        .last()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());
    let mut wide: Vec<u16> = root.encode_utf16().collect();
    if !wide.last().is_some_and(|&c| c == b'\\' as u16) {
        wide.push(b'\\' as u16);
    }
    wide.push(0);

    let (mut spc, mut bps, mut freec, mut totalc) = (0u32, 0u32, 0u32, 0u32);
    let ok = unsafe { GetDiskFreeSpaceW(wide.as_ptr(), &mut spc, &mut bps, &mut freec, &mut totalc) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let bsize = (spc as u64 * bps as u64) as f64;
    Ok(FsStat {
        fstype: 0.0,
        bsize,
        blocks: totalc as f64,
        bfree: freec as f64,
        bavail: freec as f64,
        files: 0.0,
        ffree: 0.0,
    })
}

#[cfg(not(any(unix, windows)))]
fn query(_path: &str) -> std::io::Result<FsStat> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "statfs unsupported on this platform"))
}

/// `fs.statfsSync(path)` → `{ type, bsize, blocks, bfree, bavail, files, ffree }`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATFS(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match query(&path) {
        Ok(s) => {
            let num = |v: f64| v.to_bits() as i64;
            alloc_shaped_object(
                &["type", "bsize", "blocks", "bfree", "bavail", "files", "ffree"],
                &[
                    num(s.fstype),
                    num(s.bsize),
                    num(s.blocks),
                    num(s.bfree),
                    num(s.bavail),
                    num(s.files),
                    num(s.ffree),
                ],
            )
        }
        Err(e) => {
            throw_io(&e, "statfs", &path);
            0
        }
    }
}
