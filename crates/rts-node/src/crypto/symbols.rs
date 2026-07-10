//! node:crypto — base extern "C" symbol implementations (the sync surface).
//!
//! Native rts-node implementation (no rts-std mirror; `rts:crypto` keeps its
//! own RTS-flavored surface — see `crates/rts-std/src/crypto/mod.rs`). This
//! slice covers the pure, synchronous, flat-value surface implemented so far:
//!
//! - `randomUUID()` — RFC 4122 version-4 (random) UUID. 16 bytes of real OS
//!   CSPRNG entropy via the `getrandom` crate (`getrandom::getrandom`, backed
//!   by `BCryptGenRandom` on Windows / `getrandom(2)`-or-`/dev/urandom` on
//!   Unix — the same primitive Node itself uses under libuv). The version
//!   nibble (byte 6 high nibble) is forced to `0100` and the variant (byte 8
//!   top two bits) to `10` per RFC 4122 §4.4, then formatted as lowercase
//!   `8-4-4-4-12` hex. A genuinely new random value every call — never a fixed
//!   or seeded placeholder.
//!
//! **Deferred** (need `Buffer`/`Entry::Backend`-shaped stateful objects this
//! pure-function slice does not have yet — no fake data substituted):
//! - `randomBytes(size)` / `randomFillSync(buffer)` — return/fill a `Buffer`;
//!   needs the `Buffer` handle type wired into rts-node first.
//! - `createHash(algorithm)` / `createHmac(algorithm, key)` — stateful
//!   `Hash`/`Hmac` objects with `.update()`/`.digest()`; needs a handle-backed
//!   object (`Entry::Backend` or equivalent) to hold streaming digest state.
//! - `createCipheriv` / `createDecipheriv` — stateful cipher objects; same
//!   handle-backed-object dependency, plus a real AEAD/cipher backend.
//! - `createSign` / `createVerify` — stateful signer objects; same dependency.
//!
//! ABI mirrors the pure-namespace shape used across RTS: no-arg functions
//! return a GC string handle (`intern`); symbols follow the rts-node
//! convention `__RTS_FN_NODE_CRYPTO_*`.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Formats 16 raw bytes as a lowercase RFC 4122 UUID string
/// (`8-4-4-4-12` hex groups). Does not itself enforce version/variant bits —
/// callers set those before formatting.
fn format_uuid(bytes: &[u8; 16]) -> String {
    let mut s = String::with_capacity(36);
    for (i, b) in bytes.iter().enumerate() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            s.push('-');
        }
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Generates a real RFC 4122 v4 (random) UUID from 16 CSPRNG bytes.
fn random_uuid_v4() -> String {
    let mut buf = [0u8; 16];
    // getrandom::getrandom draws real OS entropy (BCryptGenRandom on Windows,
    // getrandom(2)/`/dev/urandom` on Unix). Failure here means the OS entropy
    // source itself is broken/unavailable — there is no honest fallback value,
    // so this is the one place in this module where we propagate a panic
    // rather than fabricate a "random" UUID.
    getrandom::getrandom(&mut buf).expect("node:crypto.randomUUID: OS CSPRNG unavailable");

    // Version 4: top nibble of byte 6 = 0b0100.
    buf[6] = (buf[6] & 0x0f) | 0x40;
    // Variant (RFC 4122): top two bits of byte 8 = 0b10.
    buf[8] = (buf[8] & 0x3f) | 0x80;

    format_uuid(&buf)
}

/// `crypto.randomUUID()` — a fresh, real RFC 4122 v4 UUID every call.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_CRYPTO_RANDOM_UUID() -> u64 {
    intern(&random_uuid_v4())
}
