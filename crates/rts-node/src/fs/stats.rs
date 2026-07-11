//! node:fs — the `Stats` object returned by `statSync`/`lstatSync`. An
//! object-backed Registry class (`__rts_class = "Stats"`, the StringDecoder/Hash
//! model): the numeric fields (`size`/`mtimeMs`/…/`mode`) are stored directly on
//! the instance Map, and a `__ftype` tag drives `isFile`/`isDirectory`/
//! `isSymbolicLink`. Every value is read from the real `std::fs::Metadata`.

use std::time::UNIX_EPOCH;

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

fn time_ms(t: std::io::Result<std::time::SystemTime>) -> f64 {
    t.ok()
        .and_then(|st| st.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

#[cfg(unix)]
fn mode_of(m: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    m.mode() as i64
}

#[cfg(not(unix))]
fn mode_of(m: &std::fs::Metadata) -> i64 {
    // Windows has no POSIX mode: synthesize the read/write bits Node reports.
    if m.permissions().readonly() { 0o444 } else { 0o666 }
}

/// Build a `Stats` instance from `std::fs::Metadata`. `ftype`: 0 file / 1 dir /
/// 2 symlink.
pub fn build(m: &std::fs::Metadata) -> u64 {
    let ftype: i64 = if m.is_dir() {
        1
    } else if m.file_type().is_symlink() {
        2
    } else {
        0
    };
    let num = |v: f64| v.to_bits() as i64;
    let mut map: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    map.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"Stats".to_vec())) as i64);
    map.insert("__ftype".to_string(), ftype);
    map.insert("size".to_string(), num(m.len() as f64));
    map.insert("mode".to_string(), num(mode_of(m) as f64));
    map.insert("mtimeMs".to_string(), num(time_ms(m.modified())));
    map.insert("atimeMs".to_string(), num(time_ms(m.accessed())));
    map.insert("birthtimeMs".to_string(), num(time_ms(m.created())));
    // ctime is not portable via std; Node reports the inode-change time — use
    // mtime as the closest portable value the metadata exposes.
    map.insert("ctimeMs".to_string(), num(time_ms(m.modified())));
    alloc_entry(Entry::Map(Box::new(map)))
}

/// Read a numeric field (stored as an inline-f64 word) off a `Stats` instance.
pub fn num_field(handle: u64, key: &str) -> f64 {
    let word = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0),
        _ => 0,
    });
    f64::from_bits(word as u64)
}

fn ftype(handle: u64) -> i64 {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get("__ftype").copied().unwrap_or(-1),
        _ => -1,
    })
}

/// `stats.isFile()`.
pub fn is_file(handle: u64) -> bool {
    ftype(handle) == 0
}

/// `stats.isDirectory()`.
pub fn is_directory(handle: u64) -> bool {
    ftype(handle) == 1
}

/// `stats.isSymbolicLink()`.
pub fn is_symbolic_link(handle: u64) -> bool {
    ftype(handle) == 2
}
