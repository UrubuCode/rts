//! SHA-256, the one hash this provider offers `rustls`, via `sha2` — the
//! same crate `node:crypto`'s `hash.rs` already links (`crates.md` §4.1).
//!
//! Every cipher suite this provider lists (`mod.rs`) is SHA-256-keyed, so
//! this is the only [`rustls::crypto::hash::Hash`]/[`rustls::crypto::hmac::Hmac`]
//! pair the provider needs. SHA-384 (needed for AES-256-GCM's usual pairing
//! and for P-384 handshake signatures) is not implemented — see `mod.rs`'s
//! "Not implemented, by name" section.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

pub(crate) struct Sha256Hash;

pub(crate) static SHA256: Sha256Hash = Sha256Hash;

impl rustls::crypto::hash::Hash for Sha256Hash {
    fn start(&self) -> Box<dyn rustls::crypto::hash::Context> {
        Box::new(Sha256Context(Sha256::new()))
    }

    fn hash(&self, data: &[u8]) -> rustls::crypto::hash::Output {
        rustls::crypto::hash::Output::new(Sha256::digest(data).as_slice())
    }

    fn output_len(&self) -> usize {
        32
    }

    fn algorithm(&self) -> rustls::crypto::hash::HashAlgorithm {
        rustls::crypto::hash::HashAlgorithm::SHA256
    }
}

struct Sha256Context(Sha256);

impl rustls::crypto::hash::Context for Sha256Context {
    fn fork_finish(&self) -> rustls::crypto::hash::Output {
        rustls::crypto::hash::Output::new(self.0.clone().finalize().as_slice())
    }

    fn fork(&self) -> Box<dyn rustls::crypto::hash::Context> {
        Box::new(Sha256Context(self.0.clone()))
    }

    fn finish(self: Box<Self>) -> rustls::crypto::hash::Output {
        rustls::crypto::hash::Output::new(self.0.finalize().as_slice())
    }

    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }
}

pub(crate) struct HmacSha256;

pub(crate) static HMAC_SHA256: HmacSha256 = HmacSha256;

impl rustls::crypto::hmac::Hmac for HmacSha256 {
    fn with_key(&self, key: &[u8]) -> Box<dyn rustls::crypto::hmac::Key> {
        Box::new(HmacSha256Key(
            Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length"),
        ))
    }

    fn hash_output_len(&self) -> usize {
        32
    }
}

struct HmacSha256Key(Hmac<Sha256>);

impl rustls::crypto::hmac::Key for HmacSha256Key {
    fn sign_concat(&self, first: &[u8], middle: &[&[u8]], last: &[u8]) -> rustls::crypto::hmac::Tag {
        let mut mac = self.0.clone();
        mac.update(first);
        for chunk in middle {
            mac.update(chunk);
        }
        mac.update(last);
        rustls::crypto::hmac::Tag::new(mac.finalize().into_bytes().as_slice())
    }

    fn tag_len(&self) -> usize {
        32
    }
}
