//! Hashes criptograficos — SHA-256 inline puro Rust.
//!
//! Impl direta do pseudocodigo FIPS 180-4. Tamanho: ~150 linhas,
//! zero deps. Nao usamos o crate `sha2` porque rt_all.rs (staticlib
//! runtime) e compilado separado sem acesso as deps do crate
//! principal; impl inline garante que o simbolo esteja no binario
//! final seja em AOT ou JIT.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn sha256(input: &[u8]) -> [u8; 32] {
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = H0;
    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, &word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_SHA256_STR(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    intern(&to_hex(&sha256(slice)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_SHA256_BYTES(ptr: i64, len: i64) -> u64 {
    if ptr == 0 || len < 0 {
        return 0;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    intern(&to_hex(&sha256(slice)))
}

// ── Streaming Hash API (#289) ───────────────────────────────────────
// node:crypto.createHash(alg).update(data).digest(enc) pattern.
// Usa o crate `sha2` direto — state machine real, sem reprocessar
// buffer no digest. Suporta apenas SHA-256 v0.

use crate::namespaces::gc::handles::{Entry, HasherState, alloc_entry, with_entry_mut};
use sha2::Digest;

/// `crypto.createHash("sha256")` — aloca novo hasher streaming via sha2::Sha256.
/// Retorna 0 se algoritmo nao suportado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HASH_NEW(alg_ptr: *const u8, alg_len: i64) -> u64 {
    if alg_ptr.is_null() || alg_len < 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(alg_ptr, alg_len as usize) };
    let alg = std::str::from_utf8(bytes).unwrap_or("");
    let state = match alg {
        "sha256" | "SHA-256" | "SHA256" => HasherState::Sha256(sha2::Sha256::new()),
        _ => return 0, // outros algoritmos pendentes
    };
    alloc_entry(Entry::Hasher(Box::new(state)))
}

/// `hash.update(data: string)` — acrescenta bytes ao hasher streaming.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HASH_UPDATE_STR(
    handle: u64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    if data_ptr.is_null() || data_len < 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    update_hasher(handle, bytes)
}

/// `hash.update(buf: Buffer)` — acrescenta bytes via ptr+len cru.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HASH_UPDATE_BYTES(
    handle: u64,
    data_ptr: i64,
    data_len: i64,
) -> i64 {
    if data_ptr == 0 || data_len < 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };
    update_hasher(handle, bytes)
}

fn update_hasher(handle: u64, bytes: &[u8]) -> i64 {
    with_entry_mut(handle, |e| {
        if let Some(Entry::Hasher(state)) = e {
            match state.as_mut() {
                HasherState::Sha256(h) => h.update(bytes),
            }
            1
        } else {
            0
        }
    })
}

/// `hash.digest("hex")` — finaliza e retorna hex string handle.
/// Apos digest, o estado eh resetado (rebinda com `Sha256::new()`).
/// Reusar o handle pos-digest equivale a um hash novo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HASH_DIGEST_HEX(handle: u64) -> u64 {
    let digest_bytes = finalize_hasher(handle);
    if digest_bytes.is_empty() {
        return 0;
    }
    intern(&to_hex(&digest_bytes))
}

/// `hash.digest("base64")` — finaliza e retorna base64 string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_HASH_DIGEST_BASE64(handle: u64) -> u64 {
    let digest_bytes = finalize_hasher(handle);
    if digest_bytes.is_empty() {
        return 0;
    }
    intern(&b64_encode(&digest_bytes))
}

fn finalize_hasher(handle: u64) -> Vec<u8> {
    with_entry_mut(handle, |e| {
        let Some(Entry::Hasher(state)) = e else { return Vec::new() };
        match state.as_mut() {
            HasherState::Sha256(h) => {
                // finalize_reset zera o estado interno e retorna o digest atual.
                let d = h.finalize_reset();
                d.to_vec()
            }
        }
    })
}

/// Base64 encoder padrao (RFC 4648), com padding `=`.
fn b64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
