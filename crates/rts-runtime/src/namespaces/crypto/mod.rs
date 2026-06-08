//! `crypto` namespace — cryptographic primitives.
//!
//! SHA-256 (inline FIPS 180-4 + a streaming `sha2` hasher), hex/base64
//! encode/decode, and an OS CSPRNG. `SHA256_DIGEST` (crypto.subtle.digest) is
//! NOT a namespace member — it stays a free extern below.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use rts_abi::ty::{Handle, I64};
use rts_macro::rts_namespace;
use sha2::Digest;

use crate::namespaces::gc::handles::{Entry, HasherState, alloc_entry, with_entry, with_entry_mut};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ── Hex ──────────────────────────────────────────────────────────────────────
const HEX_ALPHA: &[u8; 16] = b"0123456789abcdef";

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX_ALPHA[((b >> 4) & 0xF) as usize] as char);
        out.push(HEX_ALPHA[(b & 0xF) as usize] as char);
    }
    out
}

// ── Base64 ─────────────────────────────────────────────────────────────────
const B64_ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Padded base64 (RFC 4648) — used for digest output.
fn b64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(B64_ALPHA[(b0 >> 2) as usize] as char);
        out.push(B64_ALPHA[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHA[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHA[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ── SHA-256 (inline FIPS 180-4) ──────────────────────────────────────────────
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

#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
    0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
    0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
    0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
    0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
    0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
    0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
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

fn finalize_hasher(handle: u64) -> Vec<u8> {
    with_entry_mut(handle, |e| {
        let Some(Entry::Hasher(state)) = e else {
            return Vec::new();
        };
        match state.as_mut() {
            HasherState::Sha256(h) => h.finalize_reset().to_vec(),
        }
    })
}

// ── OS CSPRNG ────────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(
        h_algorithm: *mut core::ffi::c_void,
        pb_buffer: *mut u8,
        cb_buffer: u32,
        dw_flags: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
fn os_random_into(buf: &mut [u8]) -> bool {
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    // SAFETY: standard winapi call; valid pointer + flags.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    status == 0
}

#[cfg(not(target_os = "windows"))]
fn os_random_into(buf: &mut [u8]) -> bool {
    use std::fs::File;
    use std::io::Read;
    match File::open("/dev/urandom") {
        Ok(mut f) => f.read_exact(buf).is_ok(),
        Err(_) => false,
    }
}

/// `crypto.subtle.digest("SHA-256", data)` — NOT a namespace member. Takes the
/// HANDLE of the input (Buffer/Vec) and returns a Promise fulfilled with a
/// Buffer of the 32 raw digest bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_SHA256_DIGEST(src: u64) -> u64 {
    let bytes: Vec<u8> = with_entry(src, |entry| match entry {
        Some(Entry::Buffer(b)) => b.clone(),
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u8).collect(),
        _ => Vec::new(),
    });
    let digest = sha256(&bytes);
    let buf_h = alloc_entry(Entry::Buffer(digest.to_vec())) as i64;
    let slot = crate::namespaces::gc::promise_slot::new_fulfilled(buf_h);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// Cryptographic primitives: SHA-256, hex/base64, CSPRNG.
#[rts_namespace(crypto)]
impl CryptoNs {
    /// Fills `len` bytes at `ptr` with CSPRNG data. 0 on success, -1 on error.
    #[rts_fn]
    pub fn random_bytes(ptr: I64, len: I64) -> I64 {
        if ptr == 0 || len <= 0 {
            return -1;
        }
        // SAFETY: caller guarantees `ptr` covers `len` writable bytes.
        let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) };
        if os_random_into(slice) { 0 } else { -1 }
    }

    /// A cryptographically secure i64 (0 is a valid value).
    #[rts_fn]
    pub fn random_i64() -> I64 {
        let mut bytes = [0u8; 8];
        if os_random_into(&mut bytes) {
            i64::from_le_bytes(bytes)
        } else {
            0
        }
    }

    /// Allocates a Buffer of `len` random bytes. Handle, or 0 on error.
    #[rts_fn]
    pub fn random_buffer(len: I64) -> Handle {
        if len < 0 {
            return 0;
        }
        let mut buf = vec![0u8; len as usize];
        if !os_random_into(&mut buf) {
            return 0;
        }
        alloc_entry(Entry::Buffer(buf))
    }

    /// UUID v4 (RFC 4122) — `crypto.randomUUID()`. String handle.
    #[rts_fn(ts = "random_uuid(): string")]
    pub fn random_uuid() -> Handle {
        let mut bytes = [0u8; 16];
        if !os_random_into(&mut bytes) {
            return 0;
        }
        bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
        bytes[8] = (bytes[8] & 0x3F) | 0x80; // variant 10
        let s = format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
            bytes[4],
            bytes[5],
            bytes[6],
            bytes[7],
            bytes[8],
            bytes[9],
            bytes[10],
            bytes[11],
            bytes[12],
            bytes[13],
            bytes[14],
            bytes[15],
        );
        alloc_entry(Entry::String(s.into_bytes()))
    }

    /// `crypto.createHash("sha256")` — new streaming hasher. 0 if unsupported.
    #[rts_fn]
    pub fn hash_new(alg: Str) -> Handle {
        let state = match alg {
            "sha256" | "SHA-256" | "SHA256" => HasherState::Sha256(sha2::Sha256::new()),
            _ => return 0,
        };
        alloc_entry(Entry::Hasher(Box::new(state)))
    }

    /// `hash.update(data: string)` — feed bytes into the streaming hasher.
    #[rts_fn]
    pub fn hash_update_str(h: Handle, data: Str) -> I64 {
        update_hasher(h, data.as_bytes())
    }

    /// `hash.update(buf)` — feed raw ptr+len bytes.
    #[rts_fn]
    pub fn hash_update_bytes(h: Handle, ptr: I64, len: I64) -> I64 {
        if ptr == 0 || len < 0 {
            return 0;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        update_hasher(h, bytes)
    }

    /// `hash.digest("hex")` — finalize, return a hex string handle.
    #[rts_fn(ts = "hash_digest_hex(h: number): string")]
    pub fn hash_digest_hex(h: Handle) -> Handle {
        let d = finalize_hasher(h);
        if d.is_empty() {
            return 0;
        }
        intern(&to_hex(&d))
    }

    /// `hash.digest("base64")` — finalize, return a base64 string handle.
    #[rts_fn(ts = "hash_digest_base64(h: number): string")]
    pub fn hash_digest_base64(h: Handle) -> Handle {
        let d = finalize_hasher(h);
        if d.is_empty() {
            return 0;
        }
        intern(&b64_encode(&d))
    }

    /// SHA-256 of a string, lowercase hex. String handle.
    #[rts_fn(ts = "sha256_str(s: string): string")]
    pub fn sha256_str(s: Str) -> Handle {
        intern(&to_hex(&sha256(s.as_bytes())))
    }

    /// SHA-256 of raw bytes (ptr+len), lowercase hex. String handle.
    #[rts_fn(ts = "sha256_bytes(ptr: number, len: number): string")]
    pub fn sha256_bytes(ptr: I64, len: I64) -> Handle {
        if ptr == 0 || len < 0 {
            return 0;
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        intern(&to_hex(&sha256(slice)))
    }

    /// Hex-encode raw bytes (ptr+len). Lowercase string handle.
    #[rts_fn(ts = "hex_encode(ptr: number, len: number): string")]
    pub fn hex_encode(ptr: I64, len: I64) -> Handle {
        if ptr == 0 || len < 0 {
            return 0;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        intern(&to_hex(bytes))
    }

    /// Hex-decode a string into a Buffer handle. 0 on error.
    #[rts_fn]
    pub fn hex_decode(s: Str) -> Handle {
        let bytes = s.as_bytes();
        if bytes.len() % 2 != 0 {
            return 0;
        }
        let mut out = Vec::with_capacity(bytes.len() / 2);
        let mut i = 0;
        while i < bytes.len() {
            let (Some(hi), Some(lo)) = (hex_digit(bytes[i]), hex_digit(bytes[i + 1])) else {
                return 0;
            };
            out.push((hi << 4) | lo);
            i += 2;
        }
        alloc_entry(Entry::Buffer(out))
    }

    /// Base64-encode raw bytes (ptr+len, RFC 4648 padded). String handle.
    #[rts_fn(ts = "base64_encode(ptr: number, len: number): string")]
    pub fn base64_encode(ptr: I64, len: I64) -> Handle {
        if ptr == 0 || len < 0 {
            return 0;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        intern(&b64_encode(bytes))
    }

    /// Base64-decode a string into a Buffer handle. 0 on error.
    #[rts_fn]
    pub fn base64_decode(s: Str) -> Handle {
        let bytes = s.as_bytes();
        if bytes.len() % 4 != 0 {
            return 0;
        }
        let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
        let mut i = 0;
        while i < bytes.len() {
            let (c0, c1, c2, c3) = (bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]);
            let (Some(v0), Some(v1)) = (b64_val(c0), b64_val(c1)) else {
                return 0;
            };
            out.push((v0 << 2) | (v1 >> 4));
            if c2 != b'=' {
                let Some(v2) = b64_val(c2) else { return 0 };
                out.push(((v1 & 0b1111) << 4) | (v2 >> 2));
                if c3 != b'=' {
                    let Some(v3) = b64_val(c3) else { return 0 };
                    out.push(((v2 & 0b11) << 6) | v3);
                }
            }
            i += 4;
        }
        alloc_entry(Entry::Buffer(out))
    }
}
