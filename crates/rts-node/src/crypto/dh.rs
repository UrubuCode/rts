//! node:crypto — X25519 (Curve25519) Diffie-Hellman key exchange, via
//! RustCrypto's `x25519-dalek`. Real cryptographic primitive — no stubs.
//! Needed by Signal-protocol-style key agreement (X3DH), which Baileys uses.

use x25519_dalek::{PublicKey, StaticSecret};

/// Generate a fresh X25519 keypair `(privateKey, publicKey)`, both 32 bytes.
pub fn generate_keypair() -> (Vec<u8>, Vec<u8>) {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("OS CSPRNG unavailable");
    let secret = StaticSecret::from(seed);
    let public = PublicKey::from(&secret);
    (secret.to_bytes().to_vec(), public.as_bytes().to_vec())
}

/// Derive the public key for a given 32-byte X25519 private key.
pub fn public_from_private(private: &[u8]) -> Result<Vec<u8>, String> {
    let arr: [u8; 32] = private.try_into().map_err(|_| "private key must be 32 bytes".to_string())?;
    let secret = StaticSecret::from(arr);
    Ok(PublicKey::from(&secret).as_bytes().to_vec())
}

/// X25519 shared-secret computation: `privateKey` (ours, 32 bytes) ×
/// `publicKey` (theirs, 32 bytes) → 32-byte shared secret.
pub fn diffie_hellman(private: &[u8], public: &[u8]) -> Result<Vec<u8>, String> {
    let priv_arr: [u8; 32] = private.try_into().map_err(|_| "private key must be 32 bytes".to_string())?;
    let pub_arr: [u8; 32] = public.try_into().map_err(|_| "public key must be 32 bytes".to_string())?;
    let secret = StaticSecret::from(priv_arr);
    let their_public = PublicKey::from(pub_arr);
    Ok(secret.diffie_hellman(&their_public).as_bytes().to_vec())
}
