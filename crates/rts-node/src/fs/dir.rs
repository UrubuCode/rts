//! node:fs — the `Dir` object returned by `opendirSync(path)`. Authored as a
//! `#[rtse::class]`: the instance IS the Rust struct (`Entry::Rtse`), holding
//! the directory path, the materialized `Dirent` entries (built once at open,
//! the same objects `readdirSync(withFileTypes)` returns), and a cursor.
//! `readSync()` returns the next `Dirent` or `null` at end; `closeSync()`
//! releases the cursor. The async `read`/`close`/`Symbol.asyncIterator` forms
//! wait on the event loop (#207).
//!
//! `Dir` is never constructed from JS (`opendirSync` builds it internally), so
//! there is no `#[rtse::ctor]`.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::alloc_rtse;

use super::dirent::entries_of;

/// A `Dir` instance: the directory path, its materialized `Dirent` handles, and
/// a read cursor.
#[rtse::class("Dir")]
#[derive(Clone)]
pub struct Dir {
    path: String,
    entries: Vec<i64>,
    cursor: usize,
}

#[rtse::class("Dir")]
impl Dir {
    /// `dir.readSync()` → the next `Dirent`, or `null` when the entries are
    /// exhausted. Advances the cursor.
    #[rtse::method]
    fn read_sync(&mut self) -> Handle {
        match self.entries.get(self.cursor).copied() {
            Some(dirent) => {
                self.cursor += 1;
                dirent as u64
            }
            // End of directory: `null` (a `0` handle reboxes to null).
            None => 0,
        }
    }

    /// `dir.closeSync()` — release the cursor (the entries were materialized at
    /// open, so there is no OS handle to close; reset the cursor to the end).
    #[rtse::method]
    fn close_sync(&mut self) {
        self.entries = Vec::new();
        self.cursor = 0;
    }

    /// `dir.path` (getter) — the directory path.
    #[rtse::getter]
    fn path(&self) -> String {
        self.path.clone()
    }
}

/// `fs.opendirSync(path)` → `Dir`. Materializes every entry up front (each a
/// `Dirent`), so `readSync` is a pure cursor walk.
///
/// Authored with `#[rtse::function]`; `fs/mod.rs` patches `THROWS` on at
/// registration (the underlying `read_dir` can fail — see `entries_of`).
#[rtse::function(module = "node:fs", value = "opendirSync", ret_ts = "Dir")]
fn opendir_sync(path: &str) -> Handle {
    let entries = entries_of(path);
    alloc_rtse(
        "Dir",
        Dir {
            path: path.to_string(),
            entries,
            cursor: 0,
        },
    )
}
