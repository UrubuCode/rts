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
//! # `digest()` is single-use, and still answers `undefined` on reuse
//!
//! Real Node throws `ERR_CRYPTO_INVALID_STATE` on a second `digest()` call.
//! This module can raise now (`entry::throw_type_error`, per rule 8 of
//! `rts-core`'s README), but the second call is left answering
//! `undefined` rather than raising: [`digest`] removes the table entry on the
//! first call — a naive "read the state" would need a placeholder, and a
//! stale placeholder answering an EMPTY digest a second time would look like
//! a real (wrong) answer running silently, which this repository's rule
//! against a paper-over refuses. Answering `undefined` instead names the
//! call as having nothing left to consume; raising a class Node does not use
//! here (`TypeError` rather than the state error) was judged the noisier of
//! two honest divergences and left as a follow-up rather than folded into
//! this pass.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

use super::digest_algo::{HashState, NAMES};
use super::util;

static TABLE: Mutex<Option<HashMap<u64, HashState>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, HashState>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[("update", update), ("digest", digest), ("copy", copy)];

/// `crypto.createHash(algorithm)`. A catchable error for a name [`NAMES`]
/// does not list, matching Node's `ERR_CRYPTO_UNSUPPORTED_OPERATION` in
/// spirit (see this crate's `crypto.md` mirror for which nine are listed;
/// `throw_type_error`'s doc says why the class raised here is `TypeError`).
pub(super) extern "C" fn create_hash(_e: u64, _this: u64, algorithm: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let built = entry::with_runtime(|context| {
        let name = util::text(context, algorithm)?;
        let state = HashState::new(&name)?;
        Some(build(context, state))
    });
    match built {
        Some(instance) => instance,
        None => {
            entry::throw_type_error("Digest method not supported");
            entry::undefined_value()
        }
    }
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

/// `hash.copy()` — a new `Hash` with the same algorithm and the same bytes
/// fed so far, independent of `this` from that point on (Node's use case is
/// taking a checkpoint mid-stream, then diverging). `HashState` derives
/// `Clone` for exactly this; the table entry `this` points at is duplicated
/// under a fresh id rather than shared, so `update()` on the copy never
/// touches the original's state.
extern "C" fn copy(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__hashId") else {
            return entry::undefined_in(context);
        };
        let cloned = with_table(|table| table.get(&id).cloned());
        match cloned {
            Some(state) => build(context, state),
            None => entry::undefined_in(context),
        }
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

/// `crypto.hash(algorithm, data, outputEncoding = 'hex')` — one-shot, no
/// `Hash` object, per §2.2.
///
/// The default is `'hex'` and not raw bytes, which is where this differs from
/// [`digest`]: `hash.digest()` with no argument answers a `Buffer`, and
/// `crypto.hash` with no third argument answers a hex STRING. Node documents
/// the asymmetry and it is not a typo — reading them as one default made
/// `crypto.hash("sha256", "abc")` answer a `Buffer` that compared unequal to
/// every hex digest beside it. `'buffer'` is how a caller asks for the bytes,
/// which [`util::digest_output`] already covers by treating an unrecognized
/// encoding name as no-encoding.
pub(super) extern "C" fn hash_oneshot(
    _e: u64,
    _this: u64,
    algorithm: u64,
    data: u64,
    output_encoding: u64,
    _a3: u64,
) -> u64 {
    let outcome = entry::with_runtime(|context| {
        let name = util::text(context, algorithm)?;
        let mut state = HashState::new(&name)?;
        let bytes = util::binary_bytes(context, data);
        state.update(&bytes);
        let digest = state.finalize();
        let encoding = util::text(context, output_encoding).unwrap_or_else(|| "hex".to_owned());
        Some(util::digest_output(context, &digest, Some(&encoding)))
    });
    match outcome {
        Some(value) => value,
        None => {
            entry::throw_type_error("Digest method not supported");
            entry::undefined_value()
        }
    }
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
