//! `getEnvironmentData(key)` / `setEnvironmentData(key, value)`.
//!
//! Split out of `mod.rs` at this crate's 500-line ceiling — a process-wide
//! table of [`Portable`] values, which is what the API is: it is read by
//! workers started AFTER the write, and it is the one piece of state in
//! `node:worker_threads` that deliberately crosses threads (everything else
//! is per-worker, in `registry`'s table).

use std::collections::HashMap;
use std::sync::Mutex;

use rts_core::entry;

use super::portable::{Portable, portable, rebuild};

static ENVIRONMENT: Mutex<Option<HashMap<String, Portable>>> = Mutex::new(None);

fn with_environment<T>(body: impl FnOnce(&mut HashMap<String, Portable>) -> T) -> T {
    let mut guard = ENVIRONMENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

pub(super) extern "C" fn get_environment_data(_e: u64, _this: u64, key: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = entry::text_of(key);
    entry::with_runtime(|context| {
        let held = name.and_then(|name| with_environment(|table| table.get(&name).cloned()));
        match held {
            Some(value) => rebuild(context, &value),
            None => entry::undefined_in(context),
        }
    })
}

pub(super) extern "C" fn set_environment_data(_e: u64, _this: u64, key: u64, value: u64, _c: u64, _d: u64) -> u64 {
    // Two separate calls, not one closure: `portable` opens its own borrows,
    // and doing that from inside a `with_runtime` already covering `key`
    // would nest them (see `portable`'s doc).
    let name = entry::text_of(key);
    let carried = portable(value, 0);
    if let Some(name) = name {
        with_environment(|table| table.insert(name, carried));
    }
    entry::undefined_value()
}
