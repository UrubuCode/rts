//! `node:crypto`'s `Cipheriv`/`Decipheriv` object, authored as a
//! `#[rtse::class]`. Same simplification `hash.rs`/`string_decoder::class`
//! made: the instance used to be an `Entry::Map` tagged `__rts_class =
//! "Cipher"` with every field (algo index, key/iv/aad/tag buffers) as its own
//! `Entry::Buffer` handle; now it just HOLDS the real [`CipherAlgo`] + byte
//! vectors.
//!
//! Unlike real Node streaming, `update()` only accumulates (returns an empty
//! Buffer); the full output comes from `final()` alone — GCM needs the whole
//! message before it can authenticate, so there is no correct per-call partial
//! output.

use rts_engine::abi::ty::{Handle, SelfHandle};

use super::cipher::{self, CipherAlgo};
use super::state::{byte_array, read_bytes};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn throw_error(kind: &str, msg: &str) {
    unsafe { __rtsadp_throw_js_error(kind.as_ptr(), kind.len() as i64, msg.as_ptr(), msg.len() as i64) };
}

/// A `Cipheriv`/`Decipheriv` instance: algorithm, key/iv, direction, the
/// accumulated input, and (GCM only) the AAD set via `setAAD` and the auth tag
/// set via `setAuthTag` (decrypt) / computed by `final()` (encrypt).
#[rtse::class("Cipher")]
#[derive(Clone)]
pub struct Cipher {
    algo: CipherAlgo,
    key: Vec<u8>,
    iv: Vec<u8>,
    is_decrypt: bool,
    buf: Vec<u8>,
    aad: Vec<u8>,
    tag: Vec<u8>,
}

impl Cipher {
    /// Built from `crypto.createCipheriv`/`createDecipheriv` (`symbols.rs`),
    /// which alloc it via `alloc_rtse` since it is never reached through `new
    /// Cipher()` in JS.
    pub fn new(algo: CipherAlgo, key: &[u8], iv: &[u8], is_decrypt: bool) -> Cipher {
        Cipher {
            algo,
            key: key.to_vec(),
            iv: iv.to_vec(),
            is_decrypt,
            buf: Vec::new(),
            aad: Vec::new(),
            tag: Vec::new(),
        }
    }
}

#[rtse::class("Cipher")]
impl Cipher {
    /// `cipher.update(data)` — accumulates the input (see module doc: the
    /// output comes from `final()` alone). Returns an empty Buffer, unlike
    /// Node's real streaming `update()`.
    #[rtse::method]
    fn update(&mut self, data: Handle) -> Handle {
        self.buf.extend(read_bytes(data));
        byte_array(&[])
    }

    /// `cipher.setAAD(buffer)` (GCM only) — returns `this` (chainable).
    #[rtse::method(name = "setAAD")]
    fn set_aad(&mut self, me: SelfHandle, data: Handle) -> Handle {
        self.aad = read_bytes(data);
        me
    }

    /// `decipher.setAuthTag(buffer)` (GCM only) — returns `this` (chainable).
    #[rtse::method(name = "setAuthTag")]
    fn set_auth_tag(&mut self, me: SelfHandle, data: Handle) -> Handle {
        self.tag = read_bytes(data);
        me
    }

    /// `cipher.getAuthTag()` (GCM encrypt only) — the 16-byte tag computed by
    /// the last `final()`.
    #[rtse::method(name = "getAuthTag")]
    fn get_auth_tag(&self) -> Handle {
        byte_array(&self.tag)
    }

    /// `cipher.final()` — runs the accumulated input through encrypt/decrypt
    /// and returns the output Buffer. For GCM encrypt, also stores the
    /// computed auth tag so a subsequent `getAuthTag()` reads it. For GCM
    /// decrypt, verifies the tag and throws on authentication failure (Node's
    /// contract). `final` is a Rust keyword, so the Rust fn is named
    /// `finish` and mapped back to the JS name `final`.
    #[rtse::method(name = "final", throws)]
    fn finish(&mut self) -> Handle {
        if self.algo.is_gcm() {
            if self.is_decrypt {
                match cipher::gcm_decrypt(self.algo, &self.key, &self.iv, &self.aad, &self.buf, &self.tag) {
                    Ok(pt) => byte_array(&pt),
                    Err(e) => {
                        throw_error("Error", &e);
                        byte_array(&[])
                    }
                }
            } else {
                match cipher::gcm_encrypt(self.algo, &self.key, &self.iv, &self.aad, &self.buf) {
                    Ok((ct, tag)) => {
                        self.tag = tag;
                        byte_array(&ct)
                    }
                    Err(e) => {
                        throw_error("Error", &e);
                        byte_array(&[])
                    }
                }
            }
        } else if self.is_decrypt {
            match cipher::cbc_decrypt(self.algo, &self.key, &self.iv, &self.buf) {
                Ok(pt) => byte_array(&pt),
                Err(e) => {
                    throw_error("Error", &e);
                    byte_array(&[])
                }
            }
        } else {
            match cipher::cbc_encrypt(self.algo, &self.key, &self.iv, &self.buf) {
                Ok(ct) => byte_array(&ct),
                Err(e) => {
                    throw_error("Error", &e);
                    byte_array(&[])
                }
            }
        }
    }
}
