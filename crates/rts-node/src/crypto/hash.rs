//! `node:crypto`'s `Hash`/`Hmac`, authored as a `#[rtse::class]`.
//!
//! Same simplification `string_decoder::class` made: the instance used to be an
//! `Entry::Map` tagged `__rts_class = "Hash"`, with the algorithm as an integer
//! index, an HMAC flag, and the accumulated input (and, for HMAC, the key) as
//! separate `Entry::Buffer` handles `update`/`digest` had to load and re-store
//! by hand. With `#[rtse::class]` the instance IS the Rust struct
//! (`Entry::Rtse`), so it just HOLDS the real `Algo` + byte vectors.

use rts_engine::abi::ty::{Handle, SelfHandle};

use super::algo::{self, Algo};
use super::state::{byte_array, read_bytes};

/// A `Hash` (or `Hmac`, when `is_hmac`) instance: the algorithm, the HMAC key
/// (empty when not HMAC), and the accumulated input. Node streams the input;
/// accumulating it and hashing at `digest()` yields identical output.
#[rtse::class("Hash")]
#[derive(Clone)]
pub struct Hash {
    algo: Algo,
    is_hmac: bool,
    key: Vec<u8>,
    buf: Vec<u8>,
}

impl Hash {
    /// Build a `Hash` (or `Hmac`, when `key` is `Some`) instance — called from
    /// `crypto.createHash`/`crypto.createHmac` (`symbols.rs`), which alloc it
    /// via `alloc_rtse` since neither is reached through `new Hash()` in JS.
    pub fn new(algo: Algo, key: Option<&[u8]>) -> Hash {
        Hash {
            algo,
            is_hmac: key.is_some(),
            key: key.map(<[u8]>::to_vec).unwrap_or_default(),
            buf: Vec::new(),
        }
    }

    /// The final digest bytes: HMAC when `is_hmac`, else a plain hash.
    fn compute(&self) -> Vec<u8> {
        if self.is_hmac {
            algo::hmac_bytes(self.algo, &self.key, &self.buf)
        } else {
            algo::hash_bytes(self.algo, &self.buf)
        }
    }
}

#[rtse::class("Hash")]
impl Hash {
    /// `hash.update(data)` — appends, returns `this` (chainable).
    #[rtse::method]
    fn update(&mut self, me: SelfHandle, data: Handle) -> Handle {
        self.buf.extend(read_bytes(data));
        me
    }

    /// `hash.update(data, inputEncoding)` — the string data is decoded per the
    /// encoding (hex/base64/latin1, default utf8) before hashing. Shares the JS
    /// name `update` with the single-arg form; the class path resolves the two
    /// by ARITY.
    #[rtse::method(name = "update")]
    fn update_enc(&mut self, me: SelfHandle, data: &str, input_encoding: &str) -> Handle {
        self.buf.extend(algo::decode_encoded(data, input_encoding));
        me
    }

    /// `hash.copy()` — a new Hash with the same algorithm and accumulated
    /// state, so it can be digested independently (Node's `Hash.copy()`).
    #[rtse::method]
    fn copy(&self) -> Self {
        self.clone()
    }

    /// `hash.digest()` → Buffer (Uint8Array-shaped) of the raw digest bytes.
    #[rtse::method]
    fn digest(&self) -> Handle {
        byte_array(&self.compute())
    }

    /// `hash.digest(encoding)` → encoded string. Shares the JS name `digest`
    /// with the zero-arg form; resolved by ARITY.
    #[rtse::method(name = "digest")]
    fn digest_enc(&self, encoding: &str) -> String {
        algo::encode(&self.compute(), encoding)
    }
}
