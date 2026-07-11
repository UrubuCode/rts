//! node:diagnostics_channel — the in-process pub/sub bus. Subscribers are
//! PolyValue FUNCTION words stored per channel name; `publish` invokes each
//! synchronously through the codegen fn-invoke bridge (the same bridge
//! EventEmitter uses). Real dispatch — no stubs.

use std::sync::{Mutex, OnceLock};

use indexmap::IndexMap;
use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::shapes::string_word;

unsafe extern "C" {
    // The codegen callback bridge: invoke a PolyValue FUNCTION word with up to
    // four PolyValue argument words + a `this`. Link-resolved from the engine.
    fn __rtsadp_fn_invoke(f: u64, a0: u64, a1: u64, a2: u64, a3: u64, this: u64) -> u64;
}

/// name → subscriber function words (in subscription order).
fn subs() -> &'static Mutex<IndexMap<String, Vec<u64>>> {
    static SUBS: OnceLock<Mutex<IndexMap<String, Vec<u64>>>> = OnceLock::new();
    SUBS.get_or_init(|| Mutex::new(IndexMap::new()))
}

/// Ensure a channel name exists in the store (idempotent).
pub fn ensure(name: &str) {
    subs().lock().unwrap().entry(name.to_string()).or_default();
}

/// `hasSubscribers(name)` — at least one active subscriber.
pub fn has_subscribers(name: &str) -> bool {
    subs().lock().unwrap().get(name).is_some_and(|v| !v.is_empty())
}

/// `subscribe(name, onMessage)`.
pub fn subscribe(name: &str, on_message: u64) {
    subs().lock().unwrap().entry(name.to_string()).or_default().push(on_message);
}

/// `unsubscribe(name, onMessage)` — removes the first matching subscriber word;
/// returns whether one was removed.
pub fn unsubscribe(name: &str, on_message: u64) -> bool {
    let mut map = subs().lock().unwrap();
    if let Some(v) = map.get_mut(name) {
        if let Some(pos) = v.iter().position(|&w| w == on_message) {
            v.remove(pos);
            return true;
        }
    }
    false
}

/// `channel.publish(message)` — invoke every subscriber with `(message, name)`,
/// synchronously, in subscription order.
pub fn publish(name: &str, message: u64) {
    let snapshot = subs().lock().unwrap().get(name).cloned().unwrap_or_default();
    if snapshot.is_empty() {
        return;
    }
    let name_word = string_word(name.as_bytes());
    let undef = rts_engine::heap::poly::POLY_UNDEFINED;
    for f in snapshot {
        unsafe { __rtsadp_fn_invoke(f, message, name_word, undef, undef, undef) };
    }
}

/// Build a `Channel` instance object (`__rts_class = "Channel"`, `__name` field).
pub fn build_channel(name: &str) -> u64 {
    ensure(name);
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("__rts_class".to_string(), alloc_entry(Entry::String(b"Channel".to_vec())) as i64);
    m.insert("__name".to_string(), alloc_entry(Entry::String(name.as_bytes().to_vec())) as i64);
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Read a `Channel` instance's name.
pub fn channel_name(handle: u64) -> String {
    let name_h = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get("__name").copied(),
        _ => None,
    });
    match name_h {
        Some(h) => with_entry(h as u64, |e| match e {
            Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
            _ => String::new(),
        }),
        None => String::new(),
    }
}
