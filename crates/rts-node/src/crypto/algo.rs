//! node:crypto — the hash-algorithm set and the digest/HMAC computations, over
//! RustCrypto's `DynDigest` so the algorithm is chosen at runtime. HMAC is the
//! standard ipad/opad construction built directly on `DynDigest` (no generic
//! hmac crate). Real cryptographic primitives — no stubs.

use digest::DynDigest;

/// The hash algorithms RTS supports (exactly what it can genuinely compute).
#[derive(Clone, Copy, PartialEq)]
pub enum Algo {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
}

impl Algo {
    /// Parse a Node algorithm name (case-insensitive). `None` = unsupported.
    pub fn parse(name: &str) -> Option<Algo> {
        match name.to_lowercase().as_str() {
            "md5" => Some(Algo::Md5),
            "sha1" => Some(Algo::Sha1),
            "sha224" => Some(Algo::Sha224),
            "sha256" => Some(Algo::Sha256),
            "sha384" => Some(Algo::Sha384),
            "sha512" => Some(Algo::Sha512),
            _ => None,
        }
    }

    pub fn index(self) -> i64 {
        match self {
            Algo::Md5 => 0,
            Algo::Sha1 => 1,
            Algo::Sha224 => 2,
            Algo::Sha256 => 3,
            Algo::Sha384 => 4,
            Algo::Sha512 => 5,
        }
    }

    pub fn from_index(i: i64) -> Algo {
        match i {
            0 => Algo::Md5,
            1 => Algo::Sha1,
            2 => Algo::Sha224,
            4 => Algo::Sha384,
            5 => Algo::Sha512,
            _ => Algo::Sha256,
        }
    }

    /// A fresh boxed hasher for this algorithm.
    fn hasher(self) -> Box<dyn DynDigest> {
        match self {
            Algo::Md5 => Box::new(md5::Md5::default()),
            Algo::Sha1 => Box::new(sha1::Sha1::default()),
            Algo::Sha224 => Box::new(sha2::Sha224::default()),
            Algo::Sha256 => Box::new(sha2::Sha256::default()),
            Algo::Sha384 => Box::new(sha2::Sha384::default()),
            Algo::Sha512 => Box::new(sha2::Sha512::default()),
        }
    }

    /// HMAC block size in bytes (64 for the 512-bit-state hashes, 128 for
    /// sha384/sha512).
    fn block_size(self) -> usize {
        match self {
            Algo::Sha384 | Algo::Sha512 => 128,
            _ => 64,
        }
    }
}

/// Single-shot digest of `data`.
pub fn hash_bytes(algo: Algo, data: &[u8]) -> Vec<u8> {
    let mut h = algo.hasher();
    h.update(data);
    h.finalize().to_vec()
}

/// HMAC(key, data) — `H((K' ^ opad) || H((K' ^ ipad) || data))` with the key
/// hashed down / zero-padded to the block size.
pub fn hmac_bytes(algo: Algo, key: &[u8], data: &[u8]) -> Vec<u8> {
    let bs = algo.block_size();
    let mut k = if key.len() > bs { hash_bytes(algo, key) } else { key.to_vec() };
    k.resize(bs, 0);
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();

    let mut inner = algo.hasher();
    inner.update(&ipad);
    inner.update(data);
    let ih = inner.finalize();

    let mut outer = algo.hasher();
    outer.update(&opad);
    outer.update(&ih);
    outer.finalize().to_vec()
}

/// PBKDF2 (RFC 2898) with `HMAC-<algo>` as the PRF: derive a `keylen`-byte key
/// from `password`/`salt` over `iterations` rounds. Real, standard construction
/// built on `hmac_bytes`.
pub fn pbkdf2(algo: Algo, password: &[u8], salt: &[u8], iterations: u32, keylen: usize) -> Vec<u8> {
    let h_len = hash_bytes(algo, &[]).len();
    let blocks = keylen.div_ceil(h_len.max(1));
    let mut dk = Vec::with_capacity(blocks * h_len);
    for i in 1..=blocks as u32 {
        // U1 = PRF(password, salt || INT32BE(i)).
        let mut salt_i = salt.to_vec();
        salt_i.extend_from_slice(&i.to_be_bytes());
        let mut u = hmac_bytes(algo, password, &salt_i);
        let mut t = u.clone();
        for _ in 1..iterations {
            u = hmac_bytes(algo, password, &u);
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= b;
            }
        }
        dk.extend_from_slice(&t);
    }
    dk.truncate(keylen);
    dk
}

/// `crypto.getHashes()` entries.
pub fn hashes() -> &'static [&'static str] {
    &["md5", "sha1", "sha224", "sha256", "sha384", "sha512"]
}

/// Encode digest bytes per a Node encoding name (`hex`/`base64`/`base64url`/
/// `latin1`/`binary`). Unknown → hex.
pub fn encode(bytes: &[u8], enc: &str) -> String {
    match enc.to_lowercase().as_str() {
        "base64" => base64(bytes, false),
        "base64url" => base64(bytes, true),
        "latin1" | "binary" => bytes.iter().map(|&b| b as char).collect(),
        _ => bytes.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

fn base64(data: &[u8], url: bool) -> String {
    const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let tbl = if url { URL } else { STD };
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(tbl[(n >> 18 & 63) as usize] as char);
        out.push(tbl[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { tbl[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { tbl[(n & 63) as usize] as char } else { '=' });
    }
    // base64url omits padding.
    if url { out.trim_end_matches('=').to_string() } else { out }
}
