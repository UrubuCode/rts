//! node:crypto — CSPRNG-backed helpers: `randomBytes`, `randomUUID`,
//! `randomInt`, and the constant-time `timingSafeEqual`. Entropy from the OS via
//! `getrandom` (BCryptGenRandom / getrandom syscall) — real randomness.

use super::state::{byte_array, read_bytes};

fn fill(buf: &mut [u8]) {
    // getrandom draws from the OS CSPRNG; a failure here is catastrophic, so we
    // surface it rather than emit predictable bytes.
    getrandom::getrandom(buf).expect("OS CSPRNG unavailable");
}

/// `crypto.randomBytes(size)` → Buffer (Uint8Array-shaped).
pub fn random_bytes(size: usize) -> u64 {
    let mut b = vec![0u8; size];
    fill(&mut b);
    byte_array(&b)
}

/// `crypto.randomUUID()` → RFC 4122 v4 string.
pub fn random_uuid() -> String {
    let mut b = [0u8; 16];
    fill(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 10
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// A uniform random integer in `[min, max)` (rejection sampling, unbiased).
pub fn random_int(min: i64, max: i64) -> i64 {
    if max <= min {
        return min;
    }
    let range = (max - min) as u64;
    let limit = u64::MAX - (u64::MAX % range);
    loop {
        let mut b = [0u8; 8];
        fill(&mut b);
        let v = u64::from_le_bytes(b);
        if v < limit {
            return min + (v % range) as i64;
        }
    }
}

/// `crypto.timingSafeEqual(a, b)` — constant-time byte comparison. Returns
/// `false` for length mismatch (Node throws; RTS reports unequal without leaking
/// via an exception here).
pub fn timing_safe_equal(a: u64, b: u64) -> bool {
    let ba = read_bytes(a);
    let bb = read_bytes(b);
    if ba.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in ba.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
