//! The block-cipher work itself — no JavaScript value crosses this file.
//!
//! # Why this is a separate file from `cipher/mod.rs`
//!
//! Everything here is a function from bytes to bytes, so it is testable without
//! a `Context` and reviewable without knowing this engine at all. `mod.rs` above
//! it is the opposite: it knows about prototypes, hidden ids and `Buffer`, and
//! nothing about AES. Mixing the two is how a padding bug ends up needing a
//! running interpreter to reproduce.
//!
//! # What answers this already, and what does not
//!
//! `tls/provider/aead.rs` also encrypts with AES-GCM, and it is NOT reused
//! here: it implements rustls's `MessageEncrypter`, whose unit is a TLS record
//! with its own sequence number and header as associated data. `createCipheriv`
//! has neither. What both call is the same `aes_gcm` crate, which is the level
//! at which the primitive is genuinely shared — one implementation of AES-GCM is
//! linked, and this file adds a second CALLER of it, not a second copy.

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, Nonce};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// The algorithms `createCipheriv` accepts here — exactly the ones this file
/// can genuinely compute. A name outside this list is refused at
/// `createCipheriv`, never approximated: `getCiphers()` answering a list and a
/// constructor accepting a name outside it would be two different claims about
/// the same set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CipherAlgo {
    Aes256Gcm,
    Aes128Gcm,
    Aes256Cbc,
    Aes128Cbc,
}

impl CipherAlgo {
    /// The names [`CipherAlgo::parse`] accepts, for `getCiphers()`.
    pub(crate) const NAMES: &'static [&'static str] =
        &["aes-128-cbc", "aes-128-gcm", "aes-256-cbc", "aes-256-gcm"];

    /// Node matches cipher names case-insensitively, so this does too.
    pub(super) fn parse(name: &str) -> Option<CipherAlgo> {
        match name.to_ascii_lowercase().as_str() {
            "aes-256-gcm" => Some(CipherAlgo::Aes256Gcm),
            "aes-128-gcm" => Some(CipherAlgo::Aes128Gcm),
            "aes-256-cbc" => Some(CipherAlgo::Aes256Cbc),
            "aes-128-cbc" => Some(CipherAlgo::Aes128Cbc),
            _ => None,
        }
    }

    pub(super) fn is_gcm(self) -> bool {
        matches!(self, CipherAlgo::Aes256Gcm | CipherAlgo::Aes128Gcm)
    }

    /// The key length in bytes the name itself promises. Checked at
    /// construction rather than left to the crate's own error, because
    /// `new_from_slice` reports "invalid length" without saying which of key or
    /// IV was wrong, and a 16-byte key handed to `aes-256-gcm` is the single
    /// most common way to write this call wrongly.
    pub(super) fn key_len(self) -> usize {
        match self {
            CipherAlgo::Aes256Gcm | CipherAlgo::Aes256Cbc => 32,
            CipherAlgo::Aes128Gcm | CipherAlgo::Aes128Cbc => 16,
        }
    }

    /// The IV length in bytes. GCM's 12 is not a limit invented here —
    /// `aes-gcm` as configured accepts exactly 12 — and a 16-byte IV (the CBC
    /// habit) quietly being a different nonce is the failure worth naming at
    /// the call rather than inside the crate.
    pub(super) fn iv_len(self) -> usize {
        if self.is_gcm() { 12 } else { 16 }
    }
}

/// AES-GCM encryption, answering `(ciphertext, 16-byte tag)` separately the way
/// Node's `final()`/`getAuthTag()` pair does. `aes_gcm` appends the tag to the
/// ciphertext, so the split here undoes that rather than computing anything.
pub(super) fn gcm_encrypt(
    algo: CipherAlgo,
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let nonce = Nonce::from_slice(iv);
    let payload = Payload { msg: plaintext, aad };
    let mut out = match algo {
        CipherAlgo::Aes256Gcm => Aes256Gcm::new_from_slice(key)
            .map_err(|e| e.to_string())?
            .encrypt(nonce, payload)
            .map_err(|e| e.to_string())?,
        CipherAlgo::Aes128Gcm => Aes128Gcm::new_from_slice(key)
            .map_err(|e| e.to_string())?
            .encrypt(nonce, payload)
            .map_err(|e| e.to_string())?,
        _ => return Err("not a GCM algorithm".to_owned()),
    };
    let tag = out.split_off(out.len().saturating_sub(16));
    Ok((out, tag))
}

/// AES-GCM decryption WITH tag verification — the two are one operation and
/// this file will not offer them apart. Node's shape (`setAuthTag`, then
/// `final()`) suggests otherwise, and a "decrypt now, check later" split is how
/// unauthenticated plaintext escapes: the caller in `mod.rs` therefore holds the
/// tag until `final()` and calls this once.
///
/// The error text is Node's own for this case, because a program matching on it
/// is matching on a string Node prints.
pub(super) fn gcm_decrypt(
    algo: CipherAlgo,
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8],
) -> Result<Vec<u8>, String> {
    let nonce = Nonce::from_slice(iv);
    let mut combined = ciphertext.to_vec();
    combined.extend_from_slice(tag);
    let payload = Payload { msg: &combined, aad };
    let failed = || "Unsupported state or unable to authenticate data".to_owned();
    match algo {
        CipherAlgo::Aes256Gcm => Aes256Gcm::new_from_slice(key)
            .map_err(|e| e.to_string())?
            .decrypt(nonce, payload)
            .map_err(|_| failed()),
        CipherAlgo::Aes128Gcm => Aes128Gcm::new_from_slice(key)
            .map_err(|e| e.to_string())?
            .decrypt(nonce, payload)
            .map_err(|_| failed()),
        _ => Err("not a GCM algorithm".to_owned()),
    }
}

/// AES-CBC with PKCS#7 padding, which is Node's default — `setAutoPadding(false)`
/// is not offered, and `mod.rs`'s "Not implemented" note says so by name.
pub(super) fn cbc_encrypt(
    algo: CipherAlgo,
    key: &[u8],
    iv: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    match algo {
        CipherAlgo::Aes256Cbc => Ok(Aes256CbcEnc::new_from_slices(key, iv)
            .map_err(|e| e.to_string())?
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext)),
        CipherAlgo::Aes128Cbc => Ok(Aes128CbcEnc::new_from_slices(key, iv)
            .map_err(|e| e.to_string())?
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext)),
        _ => Err("not a CBC algorithm".to_owned()),
    }
}

/// AES-CBC decryption, unpadding included. Bad padding is an error and not a
/// truncated answer, and `Pkcs7`'s own unpad is what decides rather than a
/// hand-written length check — that check is the half of CBC that is easy to
/// write and easy to write wrongly.
pub(super) fn cbc_decrypt(
    algo: CipherAlgo,
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let bad = |_| "error:1C800064:Provider routines::bad decrypt".to_owned();
    match algo {
        CipherAlgo::Aes256Cbc => Aes256CbcDec::new_from_slices(key, iv)
            .map_err(|e| e.to_string())?
            .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
            .map_err(bad),
        CipherAlgo::Aes128Cbc => Aes128CbcDec::new_from_slices(key, iv)
            .map_err(|e| e.to_string())?
            .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
            .map_err(bad),
        _ => Err("not a CBC algorithm".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NIST SP 800-38D test case 13 — the all-zero 256-bit key and 96-bit IV
    /// over an empty message. A round-trip alone cannot tell AES-GCM from any
    /// other reversible transform; a published vector can.
    #[test]
    fn gcm_matches_the_published_vector() {
        let (ct, tag) = gcm_encrypt(CipherAlgo::Aes256Gcm, &[0u8; 32], &[0u8; 12], &[], &[]).unwrap();
        assert!(ct.is_empty());
        assert_eq!(
            tag,
            [
                0x53, 0x0f, 0x8a, 0xfb, 0xc7, 0x45, 0x36, 0xb9, 0xa9, 0x63, 0xb4, 0xf1, 0xc4, 0xcb,
                0x73, 0x8b
            ]
        );
    }

    #[test]
    fn a_changed_tag_fails_to_authenticate() {
        let (ct, mut tag) =
            gcm_encrypt(CipherAlgo::Aes256Gcm, &[7u8; 32], &[3u8; 12], &[], b"payload").unwrap();
        tag[0] ^= 1;
        assert!(gcm_decrypt(CipherAlgo::Aes256Gcm, &[7u8; 32], &[3u8; 12], &[], &ct, &tag).is_err());
    }

    /// Associated data is authenticated but not encrypted, so decrypting under
    /// different AAD must fail even though the ciphertext is untouched.
    #[test]
    fn associated_data_is_authenticated() {
        let (ct, tag) =
            gcm_encrypt(CipherAlgo::Aes128Gcm, &[1u8; 16], &[2u8; 12], b"header", b"body").unwrap();
        assert!(gcm_decrypt(CipherAlgo::Aes128Gcm, &[1u8; 16], &[2u8; 12], b"header", &ct, &tag).is_ok());
        assert!(gcm_decrypt(CipherAlgo::Aes128Gcm, &[1u8; 16], &[2u8; 12], b"other!", &ct, &tag).is_err());
    }

    /// NIST SP 800-38A F.2.5 — the first block of the AES-256-CBC vector. The
    /// assertion is on the PREFIX because this file always pads, so a 16-byte
    /// plaintext comes back as 32 bytes of ciphertext.
    #[test]
    fn cbc_matches_the_published_vector() {
        let key = [
            0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe, 0x2b, 0x73, 0xae, 0xf0, 0x85, 0x7d,
            0x77, 0x81, 0x1f, 0x35, 0x2c, 0x07, 0x3b, 0x61, 0x08, 0xd7, 0x2d, 0x98, 0x10, 0xa3,
            0x09, 0x14, 0xdf, 0xf4,
        ];
        let iv: Vec<u8> = (0x00u8..=0x0f).collect();
        let plain = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a,
        ];
        let out = cbc_encrypt(CipherAlgo::Aes256Cbc, &key, &iv, &plain).unwrap();
        assert_eq!(
            out[..16],
            [
                0xf5, 0x8c, 0x4c, 0x04, 0xd6, 0xe5, 0xf1, 0xba, 0x77, 0x9e, 0xab, 0xfb, 0x5f, 0x7b,
                0xfb, 0xd6
            ]
        );
    }

    #[test]
    fn cbc_rejects_corrupted_padding() {
        let mut ct =
            cbc_encrypt(CipherAlgo::Aes128Cbc, &[9u8; 16], &[4u8; 16], b"twelve bytes").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0xff;
        assert!(cbc_decrypt(CipherAlgo::Aes128Cbc, &[9u8; 16], &[4u8; 16], &ct).is_err());
    }
}
