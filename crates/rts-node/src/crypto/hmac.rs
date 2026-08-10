//! `Hmac` — `createHmac(algorithm, key)`, over §2.1's `Hmac`.
//!
//! Same handle-table shape as [`super::hash`] (see that module's doc for
//! why the state cannot live on the instance, and why `digest()` removes
//! the table entry instead of throwing on reuse) — this module does not
//! restate that reasoning, only the one new piece: `hmac::Hmac<D>` needs the
//! key at CONSTRUCTION, not at digest time, so [`create_hmac`] takes it as
//! its second argument rather than through `update`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use hmac::{Hmac, KeyInit, Mac};
use rts_core::entry::{self, Context, Provided};

use super::util;

enum HmacState {
    Sha256(Hmac<sha2::Sha256>),
    Sha512(Hmac<sha2::Sha512>),
    Sha384(Hmac<sha2::Sha384>),
    Sha224(Hmac<sha2::Sha224>),
    Sha1(Hmac<sha1::Sha1>),
    Md5(Hmac<md5::Md5>),
    Ripemd160(Hmac<ripemd::Ripemd160>),
}

/// Node's `Hmac` accepts anything `createHash` does except BLAKE2b512 and
/// SHA3-256 — `hmac::Hmac<D>` needs `D: CoreProxy` over an `EagerHash` core
/// (the fixed-block Merkle–Damgård shape SHA-2/SHA-1/MD5/RIPEMD all share).
/// `blake2::Blake2b512` (a variable/keyed-hash design already) and
/// `sha3::Sha3_256` (a sponge construction, not block-eager) do not
/// implement that trait in the RustCrypto crates this vendors — verified by
/// `cargo check`, not assumed; see this module's "Not implemented" note.
///
/// # Not implemented, by name
///
/// `blake2b512` and `sha3-256` under `createHmac` — the crate-level reason
/// above. `crypto` module's own `getHashes()` still lists both (they ARE
/// valid `createHash` algorithms); only `createHmac` with either name
/// answers `undefined`.
fn new_state(name: &str, key: &[u8]) -> Option<HmacState> {
    match name.to_ascii_lowercase().as_str() {
        "sha256" => Hmac::new_from_slice(key).ok().map(HmacState::Sha256),
        "sha512" => Hmac::new_from_slice(key).ok().map(HmacState::Sha512),
        "sha384" => Hmac::new_from_slice(key).ok().map(HmacState::Sha384),
        "sha224" => Hmac::new_from_slice(key).ok().map(HmacState::Sha224),
        "sha1" => Hmac::new_from_slice(key).ok().map(HmacState::Sha1),
        "md5" => Hmac::new_from_slice(key).ok().map(HmacState::Md5),
        "ripemd160" => Hmac::new_from_slice(key).ok().map(HmacState::Ripemd160),
        _ => None,
    }
}

impl HmacState {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Sha256(state) => state.update(data),
            Self::Sha512(state) => state.update(data),
            Self::Sha384(state) => state.update(data),
            Self::Sha224(state) => state.update(data),
            Self::Sha1(state) => state.update(data),
            Self::Md5(state) => state.update(data),
            Self::Ripemd160(state) => state.update(data),
        }
    }

    fn finalize(self) -> Vec<u8> {
        match self {
            Self::Sha256(state) => state.finalize().into_bytes().to_vec(),
            Self::Sha512(state) => state.finalize().into_bytes().to_vec(),
            Self::Sha384(state) => state.finalize().into_bytes().to_vec(),
            Self::Sha224(state) => state.finalize().into_bytes().to_vec(),
            Self::Sha1(state) => state.finalize().into_bytes().to_vec(),
            Self::Md5(state) => state.finalize().into_bytes().to_vec(),
            Self::Ripemd160(state) => state.finalize().into_bytes().to_vec(),
        }
    }
}

static TABLE: Mutex<Option<HashMap<u64, HmacState>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, HmacState>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[("update", update), ("digest", digest)];

/// `crypto.createHmac(algorithm, key)`. `undefined` for an unsupported
/// algorithm (see [`new_state`]) — `key` is read as `BinaryLike`, matching
/// `Hash`.
pub(super) extern "C" fn create_hmac(_e: u64, _this: u64, algorithm: u64, key: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(name) = util::text(context, algorithm) else {
            return entry::undefined_in(context);
        };
        let key_bytes = util::binary_bytes(context, key);
        let Some(state) = new_state(&name, &key_bytes) else {
            return entry::undefined_in(context);
        };
        build(context, state)
    })
}

fn build(context: &mut Context, state: HmacState) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(id, state);
    });
    let prototype = entry::make_prototype(context, "Hmac", METHODS);
    let instance = entry::make_instance(context, prototype);
    util::put_hidden_number(context, instance, "__hmacId", id);
    instance
}

/// `hmac.update(data, inputEncoding?)` — chainable, answers `this`.
extern "C" fn update(_e: u64, this: u64, data: u64, input_encoding: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__hmacId") else {
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

/// `hmac.digest(encoding?)` — single-use the same way [`super::hash::digest`]
/// is; `undefined` on a second call.
extern "C" fn digest(_e: u64, this: u64, encoding: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__hmacId") else {
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
