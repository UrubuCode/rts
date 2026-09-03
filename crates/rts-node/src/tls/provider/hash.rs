//! SHA-256 and SHA-384, the two hashes this provider offers `rustls`, via
//! `sha2` — the same crate `node:crypto`'s `hash.rs` already links
//! (`crates.md` §4.1).
//!
//! SHA-256 keys `AES_128_GCM_SHA256`/`CHACHA20_POLY1305_SHA256`; SHA-384 keys
//! `AES_256_GCM_SHA384` (`mod.rs`). P-384 handshake-signature verification
//! still needs its own `SignatureVerificationAlgorithm` (`verify.rs`) and
//! stays on `mod.rs`'s "Not implemented" list — a hash existing here is not
//! the same claim as a signature scheme being wired.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha384};

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

/// Mechanical copy of [`Sha256Hash`] over `Sha384` — same shape, 48-byte
/// output instead of 32. See the module doc: `AES_256_GCM_SHA384` is what
/// needs this.
pub(crate) struct Sha384Hash;

pub(crate) static SHA384: Sha384Hash = Sha384Hash;

impl rustls::crypto::hash::Hash for Sha384Hash {
    fn start(&self) -> Box<dyn rustls::crypto::hash::Context> {
        Box::new(Sha384Context(Sha384::new()))
    }

    fn hash(&self, data: &[u8]) -> rustls::crypto::hash::Output {
        rustls::crypto::hash::Output::new(Sha384::digest(data).as_slice())
    }

    fn output_len(&self) -> usize {
        48
    }

    fn algorithm(&self) -> rustls::crypto::hash::HashAlgorithm {
        rustls::crypto::hash::HashAlgorithm::SHA384
    }
}

struct Sha384Context(Sha384);

impl rustls::crypto::hash::Context for Sha384Context {
    fn fork_finish(&self) -> rustls::crypto::hash::Output {
        rustls::crypto::hash::Output::new(self.0.clone().finalize().as_slice())
    }

    fn fork(&self) -> Box<dyn rustls::crypto::hash::Context> {
        Box::new(Sha384Context(self.0.clone()))
    }

    fn finish(self: Box<Self>) -> rustls::crypto::hash::Output {
        rustls::crypto::hash::Output::new(self.0.finalize().as_slice())
    }

    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }
}

/// Mechanical copy of [`HmacSha256`] over `Sha384`.
pub(crate) struct HmacSha384;

pub(crate) static HMAC_SHA384: HmacSha384 = HmacSha384;

impl rustls::crypto::hmac::Hmac for HmacSha384 {
    fn with_key(&self, key: &[u8]) -> Box<dyn rustls::crypto::hmac::Key> {
        Box::new(HmacSha384Key(
            Hmac::<Sha384>::new_from_slice(key).expect("HMAC accepts any key length"),
        ))
    }

    fn hash_output_len(&self) -> usize {
        48
    }
}

struct HmacSha384Key(Hmac<Sha384>);

impl rustls::crypto::hmac::Key for HmacSha384Key {
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
        48
    }
}

/// # Why these tests exist
///
/// This wrapper code (the `Hash`/`Context`/`Hmac`/`Key` `impl`s above) is what
/// this change wrote; `sha2`/`hmac` themselves are tested upstream, not here.
/// [`matches_sha2_directly`]/[`matches_hmac_directly`] cross-check this file's
/// output against calling those crates directly with no `rustls` types
/// involved, which isolates exactly the wrapper.
///
/// A hand-transcribed FIPS 180-4 SHA-384("abc") vector was tried here first
/// and DELETED rather than fixed: typed from memory, it came out wrong in the
/// tail end (confirmed by running it — [`matches_sha2_directly`] agreed with
/// the crate the whole time, so the mismatch was the transcription, not the
/// code), and a hand-copied hex string this crate cannot re-derive is a
/// number some future reader would have to trust rather than check — the same
/// shape of claim `docs/engine/lost-roots.md` and the entry-tax doc both warn
/// against elsewhere in this repository. The cross-crate checks below need no
/// such trust.
#[cfg(test)]
mod tests {
    use super::*;
    use rustls::crypto::hash::Hash as _;
    use rustls::crypto::hmac::Hmac as _;

    #[test]
    fn output_len_matches_actual_digest() {
        assert_eq!(SHA384.output_len(), 48);
        assert_eq!(SHA384.hash(b"the quick brown fox").as_ref().len(), 48);
    }

    #[test]
    fn matches_sha2_directly() {
        let data = b"cross-check the wrapper against sha2 directly";
        let expected = Sha384::digest(data);
        assert_eq!(SHA384.hash(data).as_ref(), expected.as_slice());
    }

    #[test]
    fn incremental_context_matches_one_shot() {
        let mut ctx = SHA384.start();
        ctx.update(b"hello ");
        ctx.update(b"world");
        assert_eq!(ctx.finish().as_ref(), SHA384.hash(b"hello world").as_ref());
    }

    #[test]
    fn hmac_matches_hmac_crate_directly() {
        let key = b"a reasonably long cross-check HMAC key";
        let data = b"authenticate this message";
        let mine = HMAC_SHA384.with_key(key).sign_concat(data, &[], &[]);

        let mut mac = Hmac::<Sha384>::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(data);
        let expected = mac.finalize().into_bytes();

        assert_eq!(mine.as_ref(), expected.as_slice());
        assert_eq!(HMAC_SHA384.hash_output_len(), 48);
    }

    #[test]
    fn hmac_sign_concat_matches_one_concatenated_update() {
        // `sign_concat`'s three pieces must MAC identically to one `update`
        // over the concatenation — the TLS 1.3 key schedule relies on this
        // (it calls `sign_concat` to avoid allocating the concatenation
        // itself), and a wrong split would answer a DIFFERENT MAC silently.
        let key = b"another cross-check key, this one for the split path";
        let (first, middle, last) = (b"prefix-".as_slice(), b"middle-".as_slice(), b"suffix".as_slice());
        let mine = HMAC_SHA384.with_key(key).sign_concat(first, &[middle], last);

        let mut mac = Hmac::<Sha384>::new_from_slice(key).unwrap();
        mac.update(first);
        mac.update(middle);
        mac.update(last);
        let expected = mac.finalize().into_bytes();

        assert_eq!(mine.as_ref(), expected.as_slice());
    }
}
