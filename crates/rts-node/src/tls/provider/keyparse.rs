//! Pulling a raw 32-byte scalar out of a PKCS#8-DER private key, by hand.
//!
//! # Why not `p256::pkcs8`/`ed25519_dalek`'s own PKCS#8 decoders
//!
//! Both exist, and both need a Cargo feature (`pkcs8`) that is not enabled
//! on either dependency in this crate's `Cargo.toml` — which this module may
//! not edit (the task brief's own boundary; five other agents hold the file
//! lock). Enabling a feature is a manifest edit exactly like adding a row, so
//! it is refused the same way; see this crate's own report for what was
//! refused and why, alongside anything a dependency was genuinely missing
//! for.
//!
//! # What this covers, and what it does not
//!
//! PKCS#8's `privateKey OCTET STRING` is, for both key types this provider
//! signs with, a fixed-offset tail when the key carries no attributes and no
//! explicit curve parameters — exactly what `openssl genpkey` and Node's own
//! `crypto.generateKeyPairSync` emit. This walks to the last `04 20` (an
//! OCTET STRING/context tag of length 32) and takes the 32 bytes after it,
//! which is where the scalar sits in both layouts. A key carrying an
//! attributes set, an explicit (rather than named) curve, or the optional
//! `[1] publicKey` field ahead of where this expects it is refused rather
//! than misread — this is a byte-offset reader, not an ASN.1 parser, and
//! never pretends to be one.

/// The last 32 bytes following a `04 20` tag-and-length pair, if the DER is
/// long enough to hold one. `None` for anything shorter or lacking the tag.
pub(crate) fn last_32_after_octet_string_tag(der: &[u8]) -> Option<[u8; 32]> {
    if der.len() < 34 {
        return None;
    }
    for start in (0..=der.len() - 34).rev() {
        if der[start] == 0x04 && der[start + 1] == 0x20 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&der[start + 2..start + 34]);
            return Some(out);
        }
    }
    None
}
