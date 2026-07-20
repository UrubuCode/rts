//! node:crypto — symmetric ciphers: AES-256/128-GCM (AEAD, RustCrypto
//! `aes-gcm`) and AES-256/128-CBC (RustCrypto `aes`+`cbc`, PKCS#7 padding).
//! Real cryptographic primitives — no stubs. Covers the `createCipheriv`/
//! `createDecipheriv` algorithms Baileys/libsignal-style code needs.

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};

/// PKCS#7-pad `plaintext` to a multiple of `block` bytes, block-encrypt in
/// place, and return the ciphertext. Avoids the `alloc`-feature convenience
/// methods, which this crate version doesn't expose.
fn cbc_encrypt_padded<E: BlockEncryptMut>(mut enc: E, plaintext: &[u8], block: usize) -> Vec<u8> {
    let pad_len = block - (plaintext.len() % block);
    let mut buf = plaintext.to_vec();
    buf.extend(std::iter::repeat_n(pad_len as u8, pad_len));
    let mut blocks: Vec<aes::cipher::generic_array::GenericArray<u8, _>> = buf.chunks_exact(block).map(aes::cipher::generic_array::GenericArray::clone_from_slice).collect();
    for b in blocks.iter_mut() {
        enc.encrypt_block_mut(b);
    }
    blocks.into_iter().flat_map(|b| b.to_vec()).collect()
}

/// Block-decrypt `ciphertext` in place and strip PKCS#7 padding. `Err` if the
/// length isn't block-aligned or the padding is invalid.
fn cbc_decrypt_padded<D: BlockDecryptMut>(mut dec: D, ciphertext: &[u8], block: usize) -> Result<Vec<u8>, String> {
    if ciphertext.is_empty() || ciphertext.len() % block != 0 {
        return Err("error:bad decrypt".to_string());
    }
    let mut blocks: Vec<aes::cipher::generic_array::GenericArray<u8, _>> = ciphertext.chunks_exact(block).map(aes::cipher::generic_array::GenericArray::clone_from_slice).collect();
    for b in blocks.iter_mut() {
        dec.decrypt_block_mut(b);
    }
    let mut out: Vec<u8> = blocks.into_iter().flat_map(|b| b.to_vec()).collect();
    let pad = *out.last().ok_or("error:bad decrypt")? as usize;
    if pad == 0 || pad > block || pad > out.len() {
        return Err("error:bad decrypt".to_string());
    }
    if out[out.len() - pad..].iter().any(|&b| b as usize != pad) {
        return Err("error:bad decrypt".to_string());
    }
    out.truncate(out.len() - pad);
    Ok(out)
}
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, Nonce};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// The cipher algorithms RTS supports (exactly what it can genuinely compute).
#[derive(Clone, Copy, PartialEq)]
pub enum CipherAlgo {
    Aes256Gcm,
    Aes128Gcm,
    Aes256Cbc,
    Aes128Cbc,
}

impl CipherAlgo {
    /// Parse a Node cipher algorithm name (case-insensitive). `None` = unsupported.
    pub fn parse(name: &str) -> Option<CipherAlgo> {
        match name.to_lowercase().as_str() {
            "aes-256-gcm" => Some(CipherAlgo::Aes256Gcm),
            "aes-128-gcm" => Some(CipherAlgo::Aes128Gcm),
            "aes-256-cbc" => Some(CipherAlgo::Aes256Cbc),
            "aes-128-cbc" => Some(CipherAlgo::Aes128Cbc),
            _ => None,
        }
    }

    pub fn is_gcm(self) -> bool {
        matches!(self, CipherAlgo::Aes256Gcm | CipherAlgo::Aes128Gcm)
    }
}

/// AES-GCM encrypt: `(ciphertext, authTag)`, tag is always 16 bytes.
/// `aad` is the optional additional authenticated data (`setAAD`).
pub fn gcm_encrypt(algo: CipherAlgo, key: &[u8], iv: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let nonce = Nonce::from_slice(iv);
    let payload = Payload { msg: plaintext, aad };
    let mut out = match algo {
        CipherAlgo::Aes256Gcm => Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?.encrypt(nonce, payload).map_err(|e| e.to_string())?,
        CipherAlgo::Aes128Gcm => Aes128Gcm::new_from_slice(key).map_err(|e| e.to_string())?.encrypt(nonce, payload).map_err(|e| e.to_string())?,
        _ => return Err("not a GCM algorithm".into()),
    };
    // aes-gcm appends the 16-byte tag to the ciphertext; split it for Node's
    // separate ciphertext/getAuthTag() model.
    let tag = out.split_off(out.len().saturating_sub(16));
    Ok((out, tag))
}

/// AES-GCM decrypt + tag verification in one step (Node's Decipheriv model:
/// `setAuthTag` then `final()` verifies). `Err` on auth failure.
pub fn gcm_decrypt(algo: CipherAlgo, key: &[u8], iv: &[u8], aad: &[u8], ciphertext: &[u8], tag: &[u8]) -> Result<Vec<u8>, String> {
    let nonce = Nonce::from_slice(iv);
    let mut combined = ciphertext.to_vec();
    combined.extend_from_slice(tag);
    let payload = Payload { msg: &combined, aad };
    match algo {
        CipherAlgo::Aes256Gcm => Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?.decrypt(nonce, payload).map_err(|_| "Unsupported state or unable to authenticate data".to_string()),
        CipherAlgo::Aes128Gcm => Aes128Gcm::new_from_slice(key).map_err(|e| e.to_string())?.decrypt(nonce, payload).map_err(|_| "Unsupported state or unable to authenticate data".to_string()),
        _ => Err("not a GCM algorithm".into()),
    }
}

/// AES-CBC encrypt with PKCS#7 padding.
pub fn cbc_encrypt(algo: CipherAlgo, key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    match algo {
        CipherAlgo::Aes256Cbc => Ok(cbc_encrypt_padded(Aes256CbcEnc::new_from_slices(key, iv).map_err(|e| e.to_string())?, plaintext, 16)),
        CipherAlgo::Aes128Cbc => Ok(cbc_encrypt_padded(Aes128CbcEnc::new_from_slices(key, iv).map_err(|e| e.to_string())?, plaintext, 16)),
        _ => Err("not a CBC algorithm".into()),
    }
}

/// AES-CBC decrypt + PKCS#7 unpadding. `Err` on bad padding (Node throws too).
pub fn cbc_decrypt(algo: CipherAlgo, key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    match algo {
        CipherAlgo::Aes256Cbc => cbc_decrypt_padded(Aes256CbcDec::new_from_slices(key, iv).map_err(|e| e.to_string())?, ciphertext, 16),
        CipherAlgo::Aes128Cbc => cbc_decrypt_padded(Aes128CbcDec::new_from_slices(key, iv).map_err(|e| e.to_string())?, ciphertext, 16),
        _ => Err("not a CBC algorithm".into()),
    }
}
