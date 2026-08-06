//! `Hash` — `createHash(algorithm)`, over §2.1's `Hash` and §2.2's `hash`/
//! `getHashes`.
//!
//! # Where the digest state lives
//!
//! Not on the JS instance — a `sha2::Sha256` mid-update is native state no
//! value in this engine's shape-and-property system can hold, the same limit
//! `fs/fd.rs`'s module doc states for a `std::fs::File`. So an instance
//! carries one hidden `__hashId` number, and [`TABLE`] is where the state
//! actually lives, keyed the same generation-free way `fs/fd.rs`/`fs/dir.rs`
//! key an open file / a directory cursor.
//!
//! # `digest()` is single-use without a throw
//!
//! Real Node throws `ERR_CRYPTO_INVALID_STATE` on a second `digest()` call.
//! This module cannot throw (see `assert.rs`'s module doc for why no native
//! entry point here can). [`digest`] removes the table entry on the first
//! call — a naive "read the state" would need a placeholder, and a stale
//! placeholder answering an EMPTY digest a second time would look like a
//! real (wrong) answer running silently, which this repository's rule
//! against a paper-over refuses. Answering `undefined` instead names the
//! call as having nothing left to consume.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core_rwk::entry::{self, Context, Provided};

use super::digest_algo::{HashState, NAMES};
use super::util;

static TABLE: Mutex<Option<HashMap<u64, HashState>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, HashState>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[("update", update), ("digest", digest)];

/// `crypto.createHash(algorithm)`. `undefined` for a name [`NAMES`] does not
/// list — see this crate's `crypto.md` mirror for which nine.
pub(super) extern "C" fn create_hash(_e: u64, _this: u64, algorithm: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(name) = util::text(context, algorithm) else {
            return entry::undefined_in(context);
        };
        let Some(state) = HashState::new(&name) else {
            return entry::undefined_in(context);
        };
        build(context, state)
    })
}

fn build(context: &mut Context, state: HashState) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(id, state);
    });
    let prototype = entry::make_prototype(context, "Hash", METHODS);
    let instance = entry::make_instance(context, prototype);
    util::put_hidden_number(context, instance, "__hashId", id);
    instance
}

/// `hash.update(data, inputEncoding?)` — chainable, answers `this`.
extern "C" fn update(_e: u64, this: u64, data: u64, input_encoding: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__hashId") else {
            return this;
        };
        let encoding = util::text(context, input_encoding);
        let bytes = util::binary_bytes_encoded(context, data, encoding.as_deref());
        with_table(|table| {
            if let Some(state) = table.get_mut(&id) {
                state.update(&bytes);
            }
        });
        this
    })
}

/// `hash.digest(encoding?)` — `Uint8Array` (see this module's doc) with no
/// encoding, a `string` with one. `undefined` if already consumed.
extern "C" fn digest(_e: u64, this: u64, encoding: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__hashId") else {
            return entry::undefined_in(context);
        };
        let state = with_table(|table| table.remove(&id));
        let Some(state) = state else {
            return entry::undefined_in(context);
        };
        let bytes = state.finalize();
        let encoding = util::text(context, encoding);
        util::digest_output(context, &bytes, encoding.as_deref())
    })
}

/// `crypto.hash(algorithm, data, outputEncoding?)` — one-shot, no `Hash`
/// object, per §2.2. `'buffer'` and an absent third argument both mean "raw
/// bytes" — [`util::digest_output`] already treats an unrecognized encoding
/// name as no-encoding, which covers `'buffer'` without a separate branch.
pub(super) extern "C" fn hash_oneshot(
    _e: u64,
    _this: u64,
    algorithm: u64,
    data: u64,
    output_encoding: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        let Some(name) = util::text(context, algorithm) else {
            return entry::undefined_in(context);
        };
        let Some(mut state) = HashState::new(&name) else {
            return entry::undefined_in(context);
        };
        let bytes = util::binary_bytes(context, data);
        state.update(&bytes);
        let digest = state.finalize();
        let encoding = util::text(context, output_encoding);
        util::digest_output(context, &digest, encoding.as_deref())
    })
}

/// `crypto.getHashes()` — exactly [`NAMES`], nothing OpenSSL would also list
/// (SHAKE, SHA3-224/384/512, BLAKE2s, …) — see `digest_algo.rs`'s own "Not
/// implemented" note for why those are absent rather than approximated.
pub(super) extern "C" fn get_hashes(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let values = NAMES.iter().map(|name| entry::make_string(context, name)).collect();
        entry::make_array_in(context, values)
    })
}
