//! node:fs — the `Dirent` object returned by `readdirSync(path, { withFileTypes:
//! true })`. An object-backed Registry class (`__rts_class = "Dirent"`, the
//! Stats/StringDecoder model): `name`/`parentPath`/`path` string fields plus a
//! `__ftype` tag (0 file / 1 dir / 2 symlink / 3 block / 4 char / 5 fifo /
//! 6 socket) driving the seven `is*` predicates. Every value comes from the real
//! `std::fs::DirEntry` (`file_type()` — cheap `d_type` on POSIX, metadata on
//! Windows). Now dispatchable on any receiver (the object-backed runtime dispatch
//! path), so `readdirSync(dir, { withFileTypes: true })[i].isFile()` works.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

use super::words::{opt_bool, read, string_array, throw_io};

/// Store a string field as a GC `Entry::String` (a real string handle the object
/// getters / property reads resolve), matching the Stats/StringDecoder model.
fn str_field(s: &str) -> i64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec())) as i64
}

/// Classify a `std::fs::FileType` into the `__ftype` tag. The block/char/fifo/
/// socket kinds exist only on Unix; elsewhere a non-dir/non-symlink is a regular
/// file.
fn ftype_of(ft: std::fs::FileType) -> i64 {
    if ft.is_dir() {
        return 1;
    }
    if ft.is_symlink() {
        return 2;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if ft.is_block_device() {
            return 3;
        }
        if ft.is_char_device() {
            return 4;
        }
        if ft.is_fifo() {
            return 5;
        }
        if ft.is_socket() {
            return 6;
        }
    }
    0
}

/// Build one `Dirent` instance from a name, its parent directory, and its type.
fn build(name: &str, parent: &str, ftype: i64) -> u64 {
    let mut map: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    map.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"Dirent".to_vec())) as i64);
    map.insert("__ftype".to_string(), ftype);
    map.insert("name".to_string(), str_field(name));
    map.insert("parentPath".to_string(), str_field(parent));
    // `path` is the (deprecated) alias of `parentPath`.
    map.insert("path".to_string(), str_field(parent));
    alloc_entry(Entry::Map(Box::new(map)))
}

fn ftype(handle: u64) -> i64 {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get("__ftype").copied().unwrap_or(-1),
        _ => -1,
    })
}

fn field(handle: u64, key: &str) -> u64 {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// Build the `Dirent` handle words for every entry of `path` (shared by
/// `readdirSync(withFileTypes)` and `opendirSync`). Raises the IO error and
/// returns an empty list on failure.
pub(super) fn entries_of(path: &str) -> Vec<i64> {
    match std::fs::read_dir(path) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let ft = e.file_type().map(ftype_of).unwrap_or(0);
                build(&name, path, ft) as i64
            })
            .collect(),
        Err(e) => {
            throw_io(&e, "scandir", path);
            Vec::new()
        }
    }
}

/// `fs.readdirSync(path, { withFileTypes: true })` → `Dirent[]`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READDIR_TYPES(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    alloc_entry(Entry::Vec(Box::new(entries_of(&path))))
}

/// `fs.readdirSync(path, options)` — `Dirent[]` when `options.withFileTypes` is
/// truthy, else the plain `string[]` (the `encoding`/other options do not change
/// the name strings this returns).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READDIR_OPTS(p: *const u8, l: i64, options: u64) -> u64 {
    if opt_bool(options, "withFileTypes") {
        return __RTS_FN_NODE_FS_READDIR_TYPES(p, l);
    }
    let path = read(p, l);
    match std::fs::read_dir(&path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            string_array(&names)
        }
        Err(e) => {
            throw_io(&e, "scandir", &path);
            string_array(&[])
        }
    }
}

/// `dirent.isFile()` / `isDirectory()` / `isSymbolicLink()` / `isBlockDevice()` /
/// `isCharacterDevice()` / `isFIFO()` / `isSocket()` — compare the `__ftype` tag.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_FILE(this: u64) -> i64 {
    (ftype(this) == 0) as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_DIRECTORY(this: u64) -> i64 {
    (ftype(this) == 1) as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_SYMLINK(this: u64) -> i64 {
    (ftype(this) == 2) as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_BLOCK(this: u64) -> i64 {
    (ftype(this) == 3) as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_CHAR(this: u64) -> i64 {
    (ftype(this) == 4) as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_FIFO(this: u64) -> i64 {
    (ftype(this) == 5) as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_IS_SOCKET(this: u64) -> i64 {
    (ftype(this) == 6) as i64
}

/// `dirent.name` / `.parentPath` / `.path` (getters) — the stored string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_NAME(this: u64) -> u64 {
    field(this, "name")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIRENT_PARENT_PATH(this: u64) -> u64 {
    field(this, "parentPath")
}
