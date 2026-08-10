//! The three key derivations as PURE Rust — bytes in, bytes out, no runtime.
//!
//! # Why this file has no `u64` in it
//!
//! Because a KDF that "looks right" is exactly how the wrong bytes got here in
//! the first place (`NODE_COMPATIBLE.md` §crypto: "`pbkdf2Sync` ignores the
//! `digest` argument", "`scryptSync` is not scrypt", "`hkdfSync` returns 0
//! bytes"). A derivation entangled with argument reading can only be checked by
//! running a JavaScript program; a derivation that is a function from slices to
//! a `Vec<u8>` can be pinned by an RFC vector in a `#[test]`, which is what
//! [`super::vectors`] does. Every published vector this crate claims is asserted
//! against a call in THIS file, and the glue in [`super::sync`] adds no
//! arithmetic of its own.
//!
//! # Refusal instead of zeros
//!
//! Each function answers `Result<Vec<u8>, String>`, and the `Err` is the message
//! the caller raises. The version this replaces returned a zero-filled buffer
//! for an unknown digest name, for a `keylen` past what HKDF can expand to, and
//! for scrypt parameters the crate refused — three different mistakes wearing
//! one plausible answer. A zero key opens nothing and says nothing; a throw ends
//! the call at the call.

use hkdf::Hkdf;

/// Node's default scrypt cost, from `crypto.md`'s `ScryptOptions`: `N = 16384`.
pub(super) const SCRYPT_DEFAULT_N: u64 = 16384;
/// Node's default scrypt block size.
pub(super) const SCRYPT_DEFAULT_R: u32 = 8;
/// Node's default scrypt parallelisation.
pub(super) const SCRYPT_DEFAULT_P: u32 = 1;
/// Node's default `maxmem`, 32 MiB — the ceiling `128 * N * r` is checked
/// against before any memory is reserved.
pub(super) const SCRYPT_DEFAULT_MAXMEM: u64 = 32 * 1024 * 1024;

/// Every digest name the two HMAC-based constructions here dispatch over.
///
/// # Reuse-check (`.claude/skills/reuse-check`)
///
/// Nothing in `rts-cranelift` or `rts-core-rwk` answers "which hash does this
/// name mean" — the machine has no notion of a digest and the runtime has no
/// crypto. The nearest existing answer is `super::super::digest_algo::NAMES`,
/// which is `createHash`'s FIFTEEN, and it is deliberately not reused here: it
/// includes SHA3, BLAKE2 and the SHA-512 truncations, and `hmac::Hmac<D>` needs
/// `D: EagerHash` — the fixed-block Merkle–Damgård shape a sponge and a keyed
/// hash do not have. `super::super::hmac::new_state` dispatches over exactly the
/// seven below for that reason, verified there by compilation rather than by
/// reading. Both PBKDF2 and HKDF are HMAC underneath, so a name `Hmac<D>` cannot
/// be built over is a name neither of these can use either.
///
/// This is a list of NAMES, not a second numbering of anything — reuse-check §3
/// does not apply. A name absent from it is refused by name, never defaulted:
/// Node made `digest` mandatory precisely because the old SHA-1 default silently
/// downgraded callers.
pub(super) const DIGESTS: &[&str] = &["sha256", "sha512", "sha384", "sha224", "sha1", "md5", "ripemd160"];

/// Runs `$body!(DigestType)` for a named digest, or `None`.
///
/// A macro rather than an enum: `pbkdf2_hmac` and `Hkdf` are both generic over
/// the digest TYPE and neither takes an instantiated value, so an enum would
/// need a variant carrying something that does not exist. The arms are
/// [`DIGESTS`] and must stay in step with it — `digest_names_dispatch` in
/// [`super::vectors`] fails if they drift.
macro_rules! over_digest {
    ($name:expr, $body:ident) => {
        match $name.to_ascii_lowercase().as_str() {
            "sha256" => Some($body!(sha2::Sha256)),
            "sha512" => Some($body!(sha2::Sha512)),
            "sha384" => Some($body!(sha2::Sha384)),
            "sha224" => Some($body!(sha2::Sha224)),
            "sha1" => Some($body!(sha1::Sha1)),
            "md5" => Some($body!(md5::Md5)),
            "ripemd160" => Some($body!(ripemd::Ripemd160)),
            _ => None,
        }
    };
}

/// The message raised for a digest neither construction knows.
///
/// It NAMES the supported set, which is why [`DIGESTS`] exists as data rather
/// than only as the macro's arms: `createHash` accepts fifteen names and these
/// accept seven, so "not supported" without the list sends a caller looking for
/// a typo in a name that is spelled correctly and simply cannot be HMAC'd.
fn unknown_digest(name: &str) -> String {
    format!("Digest method not supported: {name} (supported: {})", DIGESTS.join(", "))
}

/// PBKDF2-HMAC-`digest`, RFC 2898 / RFC 8018.
///
/// Pinned by RFC 6070 (SHA-1) in [`super::vectors`], and by the SHA-256 value
/// `NODE_COMPATIBLE.md` itself recorded — that pair is the whole point: the two
/// digests used to answer the same bytes, so a test that only checked one could
/// not see the bug.
///
/// `rounds` of zero is refused rather than clamped. Node raises `ERR_OUT_OF_RANGE`
/// for it, and `pbkdf2_hmac` with zero rounds answers the unstretched HMAC — a
/// value that is derivable, plausible, and not a key.
pub(super) fn pbkdf2_bytes(
    digest: &str,
    password: &[u8],
    salt: &[u8],
    rounds: u32,
    len: usize,
) -> Result<Vec<u8>, String> {
    if rounds == 0 {
        return Err("The value of \"iterations\" is out of range. It must be >= 1.".to_owned());
    }
    if len > i32::MAX as usize {
        return Err("The value of \"keylen\" is out of range.".to_owned());
    }
    // Answered before the dispatch so a zero-length request still REFUSES an
    // unknown digest: Node validates the name whatever the length.
    let mut out = vec![0u8; len];
    macro_rules! derive {
        ($d:ty) => {
            pbkdf2::pbkdf2_hmac::<$d>(password, salt, rounds, &mut out)
        };
    }
    match over_digest!(digest, derive) {
        Some(()) => Ok(out),
        None => Err(unknown_digest(digest)),
    }
}

/// scrypt, RFC 7914 §2, with Node's parameter validation.
///
/// `n` is Node's `N` — the COST, not its logarithm. The conversion is
/// `trailing_zeros`, guarded by `is_power_of_two`, and not `log2().round()`:
/// rounding accepts `N = 1000` and silently derives at `N = 1024`, which is a
/// key nobody asked for that no error mentions. RFC 7914 §2 and Node both
/// require a power of two greater than one.
///
/// Pinned by RFC 7914 §12 vectors 1 and 2 in [`super::vectors`]. Vector 2
/// (`N = 1024, r = 8, p = 16`) is the one that pins the parameters as LIVE:
/// `NODE_COMPATIBLE.md` recorded `{N:16,r:1,p:1}` and `{N:1024,r:8,p:16}`
/// producing identical bytes, and vector 1 alone could not have seen that.
pub(super) fn scrypt_bytes(
    password: &[u8],
    salt: &[u8],
    n: u64,
    r: u32,
    p: u32,
    maxmem: u64,
    len: usize,
) -> Result<Vec<u8>, String> {
    let invalid = "Invalid scrypt params".to_owned();
    if n <= 1 || !n.is_power_of_two() || r == 0 || p == 0 {
        return Err(invalid);
    }
    // The ceiling is checked in `u128` and BEFORE `Params::new`, which reserves
    // `128 * N * r` bytes as soon as it is used. A `maxmem` that a caller set to
    // protect the process is worthless if the allocation happens first.
    let required = 128u128 * u128::from(n) * u128::from(r);
    if required > u128::from(maxmem) {
        return Err(invalid);
    }
    if len > i32::MAX as usize {
        return Err("The value of \"keylen\" is out of range.".to_owned());
    }
    // `scrypt::scrypt` refuses an empty output slice, and Node answers a
    // zero-length Buffer. Answered here rather than letting the crate's refusal
    // become a throw the caller did not earn.
    if len == 0 {
        return Ok(Vec::new());
    }
    let log_n = n.trailing_zeros() as u8;
    let params = scrypt::Params::new(log_n, r, p).map_err(|_| invalid.clone())?;
    let mut out = vec![0u8; len];
    scrypt::scrypt(password, salt, &params, &mut out).map_err(|_| invalid)?;
    Ok(out)
}

/// HKDF-Extract-then-Expand, RFC 5869 §2.
///
/// Pinned by RFC 5869 §A test cases 1, 3 and 4 in [`super::vectors`] — 3 is the
/// zero-length salt and info case and 4 is SHA-1, so the `digest` argument is
/// pinned as live here the same way it is for PBKDF2.
///
/// An empty `salt` is passed as `Some(&[])` and not `None`. HMAC pads a short
/// key with zeros to the block size, so an empty key and RFC 5869's
/// "HashLen zeros" salt are the same extraction — test case 3 asserts that
/// rather than the comment claiming it.
///
/// `len` past `255 * HashLen` is refused by the crate and reported, not
/// truncated: RFC 5869 §2.3 makes that length unrepresentable, and the previous
/// version answered `len` zero bytes for it.
pub(super) fn hkdf_bytes(
    digest: &str,
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>, String> {
    if len > i32::MAX as usize {
        return Err("The value of \"keylen\" is out of range.".to_owned());
    }
    let mut out = vec![0u8; len];
    macro_rules! expand {
        ($d:ty) => {
            Hkdf::<$d>::new(Some(salt), ikm).expand(info, &mut out).is_ok()
        };
    }
    match over_digest!(digest, expand) {
        Some(true) => Ok(out),
        Some(false) => Err("The value of \"keylen\" is out of range.".to_owned()),
        None => Err(unknown_digest(digest)),
    }
}
