//! The DICTIONARY object path — `Entry::Map(Box<IndexMap<String, i64>>)`.
//!
//! This is the representation `__rtsadp_obj_get` reaches for on any receiver
//! that is not a shaped `Entry::Vec` (`objops.rs:290-338`): object-backed
//! Registry-class instances, runtime-built rows, anything untracked. The step
//! that is easy to miss is `key_text(key_str_handle)` — it resolves the KEY's own
//! handle into an owned `String` before the receiver's lock is taken, so a
//! dictionary read allocates a `String` PER READ.

use crate::slab::{self, Entry};

/// The real path: resolve the key handle to an owned `String` (its own lock +
/// an allocation), then look it up in the map under the receiver's lock.
#[inline(never)]
pub extern "C" fn probe_dict_get(obj_payload: i64, key_payload: i64) -> i64 {
    let key: String = slab::sharded::with(key_payload as u64, |e| match e {
        Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
        _ => String::new(),
    });
    slab::sharded::with(obj_payload as u64, |e| match e {
        Some(Entry::Map(m)) => m.get(&key).copied().unwrap_or(0),
        _ => 0,
    })
}

/// Same dictionary, same lock, but the key is already an interned `&'static str`
/// — no per-read `String` allocation. Isolates what `key_text` costs from what
/// the hash map costs.
#[inline(never)]
pub extern "C" fn probe_dict_get_borrowed(obj_payload: i64, key_ptr: i64) -> i64 {
    // SAFETY: the driver passes a pointer to a `&'static str` it owns.
    let key: &str = unsafe { &*(key_ptr as *const &str) };
    slab::sharded::with(obj_payload as u64, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0),
        _ => 0,
    })
}

pub fn new_dict(entries: &[(&str, i64)]) -> u64 {
    let mut m = indexmap::IndexMap::new();
    for (k, v) in entries {
        m.insert((*k).to_string(), *v);
    }
    slab::sharded::alloc(Entry::Map(Box::new(m)))
}
