//! node:fs — the `Dir` object returned by `opendirSync(path)`. An object-backed
//! Registry class (`__rts_class = "Dir"`) holding the directory path, the
//! materialized `Dirent` entries (built once at open, the same objects
//! `readdirSync(withFileTypes)` returns), and a cursor. `readSync()` returns the
//! next `Dirent` or `null` at end; `closeSync()` releases the cursor. The async
//! `read`/`close`/`Symbol.asyncIterator` forms wait on the event loop (#207).

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

use super::dirent::entries_of;
use super::words::read;

fn str_handle(s: &str) -> i64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec())) as i64
}

/// `fs.opendirSync(path)` → `Dir`. Materializes every entry up front (each a
/// `Dirent`), so `readSync` is a pure cursor walk.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_OPENDIR(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    let entries = entries_of(&path);
    let entries_handle = alloc_entry(Entry::Vec(Box::new(entries))) as i64;
    let mut map: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    map.insert("__rts_class".to_string(), str_handle("Dir"));
    map.insert("path".to_string(), str_handle(&path));
    map.insert("__entries".to_string(), entries_handle);
    map.insert("__cursor".to_string(), 0);
    alloc_entry(Entry::Map(Box::new(map)))
}

fn field(handle: u64, key: &str) -> i64 {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0),
        _ => 0,
    })
}

/// `dir.readSync()` → the next `Dirent`, or `null` when the entries are
/// exhausted. Advances the cursor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIR_READ(this: u64) -> u64 {
    let cursor = field(this, "__cursor");
    let entries = field(this, "__entries") as u64;
    let ent = with_entry(entries, |e| match e {
        Some(Entry::Vec(v)) => v.get(cursor as usize).copied(),
        _ => None,
    });
    match ent {
        Some(dirent) => {
            with_entry_mut(this, |e| {
                if let Some(Entry::Map(m)) = e {
                    m.insert("__cursor".to_string(), cursor + 1);
                }
            });
            dirent as u64
        }
        // End of directory: `null` (a `0` handle reboxes to null).
        None => 0,
    }
}

/// `dir.closeSync()` — release the cursor (the entries were materialized at open,
/// so there is no OS handle to close; reset the cursor to the end).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIR_CLOSE(this: u64) {
    with_entry_mut(this, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert("__entries".to_string(), alloc_entry(Entry::Vec(Box::new(Vec::new()))) as i64);
            m.insert("__cursor".to_string(), 0);
        }
    });
}

/// `dir.path` (getter) — the stored directory path string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_DIR_PATH(this: u64) -> u64 {
    field(this, "path") as u64
}
