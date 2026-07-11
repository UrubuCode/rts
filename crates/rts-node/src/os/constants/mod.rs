//! node:os — `os.constants` (a `Constant` property getter returning the nested
//! `{ signals, errno, dlopen, priority, libuv }` object).
//!
//! Every value is a **real** platform constant: POSIX signal/errno numbers come
//! from the `libc` crate for the compiled target; Windows signal/errno numbers
//! are the fixed `<signal.h>`/`<errno.h>`/`<winsock2.h>` values. `priority` and
//! `libuv` are Node-level conventions (platform-independent). Constants that do
//! not exist on the compiled platform are **omitted** (never zero-filled),
//! matching Node.

mod errno;
mod other;
mod signals;

use super::words::{num_word, object_word};

/// `os.constants` — the assembled constants object.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_CONSTANTS() -> u64 {
    let signals = group_object(signals::entries());
    let errno = group_object(errno::entries());
    let dlopen = group_object(other::dlopen_entries());
    let priority = group_object(other::priority_entries());
    let libuv = group_object(other::libuv_entries());

    let keys: &[&str] = &["signals", "errno", "dlopen", "priority", "libuv"];
    let values: [i64; 5] = [signals, errno, dlopen, priority, libuv];
    super::words::object(keys, &values)
}

/// Build one nested constant sub-object from `(name, value)` pairs, returned as
/// an OBJECT element word for nesting into the parent `constants` object.
fn group_object(entries: Vec<(&'static str, i64)>) -> i64 {
    let keys: Vec<&str> = entries.iter().map(|(k, _)| *k).collect();
    let values: Vec<i64> = entries.iter().map(|(_, v)| num_word(*v as f64)).collect();
    object_word(&keys, &values)
}
