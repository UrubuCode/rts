//! node:fs — the `Dirent` object returned by `readdirSync(path, { withFileTypes:
//! true })`. Authored as a `#[rtse::class]`: the instance IS the Rust struct
//! (`Entry::Rtse`), holding `name`/`parent`/`ftype` (0 file / 1 dir / 2 symlink /
//! 3 block / 4 char / 5 fifo / 6 socket) directly instead of a flattened
//! `Entry::Map` with a `__rts_class`/`__ftype` side-tag. Every value comes from
//! the real `std::fs::DirEntry` (`file_type()` — cheap `d_type` on POSIX,
//! metadata on Windows).
//!
//! `Dirent` is never constructed from JS (`readdirSync`/`opendirSync` build it
//! internally), so there is no `#[rtse::ctor]` — `Dirent::new(...)` here is a
//! plain Rust fn, and `rts_engine::heap::handles::alloc_rtse` (the same
//! allocation path a ctor would generate) turns it into the `u64` handle callers
//! already expect.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::alloc_rtse;

use super::words::{opt_bool, string_array, throw_io};

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

/// A `Dirent` instance: name, parent directory, and the `__ftype` tag.
#[rtse::class("Dirent")]
#[derive(Clone)]
pub struct Dirent {
    name: String,
    parent: String,
    ftype: i64,
}

#[rtse::class("Dirent")]
impl Dirent {
    /// `dirent.isFile()` / `isDirectory()` / `isSymbolicLink()` / `isBlockDevice()`
    /// / `isCharacterDevice()` / `isFIFO()` / `isSocket()` — compare the `ftype`
    /// tag. `isSymbolicLink`/`isBlockDevice`/`isCharacterDevice`/`isFIFO` need an
    /// explicit JS name where the default `to_camel(rust_ident)` wouldn't produce
    /// Node's actual spelling.
    #[rtse::method]
    fn is_file(&self) -> bool {
        self.ftype == 0
    }
    #[rtse::method]
    fn is_directory(&self) -> bool {
        self.ftype == 1
    }
    #[rtse::method(name = "isSymbolicLink")]
    fn is_symlink(&self) -> bool {
        self.ftype == 2
    }
    #[rtse::method(name = "isBlockDevice")]
    fn is_block(&self) -> bool {
        self.ftype == 3
    }
    #[rtse::method(name = "isCharacterDevice")]
    fn is_char(&self) -> bool {
        self.ftype == 4
    }
    #[rtse::method(name = "isFIFO")]
    fn is_fifo(&self) -> bool {
        self.ftype == 5
    }
    #[rtse::method]
    fn is_socket(&self) -> bool {
        self.ftype == 6
    }

    /// `dirent.name`.
    #[rtse::getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    /// `dirent.parentPath`.
    #[rtse::getter]
    fn parent_path(&self) -> String {
        self.parent.clone()
    }

    /// `dirent.path` — the (deprecated) alias of `parentPath`.
    #[rtse::getter(name = "path")]
    fn path_alias(&self) -> String {
        self.parent.clone()
    }
}

/// Build one `Dirent` instance from a name, its parent directory, and its type.
fn build(name: &str, parent: &str, ftype: i64) -> u64 {
    alloc_rtse(
        "Dirent",
        Dirent {
            name: name.to_string(),
            parent: parent.to_string(),
            ftype,
        },
    )
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
fn readdir_types(path: &str) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    alloc_entry(Entry::Vec(Box::new(entries_of(path))))
}

/// `fs.readdirSync(path, options)` — `Dirent[]` when `options.withFileTypes` is
/// truthy, else the plain `string[]` (the `encoding`/other options do not change
/// the name strings this returns). Paired with `symbols::readdir_sync` (no
/// `overload` there — the base form) under the same JS name `readdirSync`.
///
/// Authored with `#[rtse::function]`; `fs/mod.rs` patches `THROWS` on at
/// registration.
#[rtse::function(module = "node:fs", value = "readdirSync", overload = "opts")]
fn readdir_sync_opts(path: &str, options: Handle) -> Handle {
    if opt_bool(options, "withFileTypes") {
        return readdir_types(path);
    }
    match std::fs::read_dir(path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            string_array(&names)
        }
        Err(e) => {
            throw_io(&e, "scandir", path);
            string_array(&[])
        }
    }
}
