//! Published test vectors for every KDF this module claims to implement.
//!
//! # What these pin, and why they are Rust tests and not `.test.ts`
//!
//! A `.test.ts` file checks the whole path — argument reading, the derivation,
//! the `Buffer` — and needs a built engine to run. These check the DERIVATION
//! alone, and they run under `cargo test -p rts-node-rwk`, which is the loop a
//! change to `derive.rs` is made in. Both are wanted; only one of them is cheap
//! enough to be run every time, and the wrong bytes this module shipped were
//! wrong in the derivation.
//!
//! Every expected value below is copied from the RFC named beside it. None is
//! computed by this crate and pasted back, which is the failure mode that would
//! make the whole file a test asserting that our code does what our code does.

use super::derive::{hkdf_bytes, pbkdf2_bytes, scrypt_bytes};

/// Decodes a hex literal from an RFC, panicking on anything that is not hex.
///
/// A test helper and not a `util` addition: `entry::decode_bytes` is the
/// crate's one codec and it goes the other way (bytes → text). Nothing here
/// ships.
fn hex(text: &str) -> Vec<u8> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(cleaned.len() % 2 == 0, "hex literal has an odd length");
    (0..cleaned.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&cleaned[at..at + 2], 16).expect("hex literal"))
        .collect()
}

fn hexed(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------------------------------------------------------- PBKDF2 ---

/// RFC 6070 §2 — every PBKDF2-HMAC-SHA-1 vector the RFC states, except the
/// `c = 16777216` one, which the RFC itself marks as taking a long time and
/// which would dominate a debug-build test run.
#[test]
fn pbkdf2_sha1_matches_rfc_6070() {
    let cases: &[(&[u8], &[u8], u32, usize, &str)] = &[
        (b"password", b"salt", 1, 20, "0c60c80f961f0e71f3a9b524af6012062fe037a6"),
        (b"password", b"salt", 2, 20, "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957"),
        (b"password", b"salt", 4096, 20, "4b007901b765489abead49d926f721d065a429c1"),
        (
            b"passwordPASSWORDpassword",
            b"saltSALTsaltSALTsaltSALTsaltSALTsalt",
            4096,
            25,
            "3d2eec4fe41c849b80c8d83662c0e44a8b291a964cf2f07038",
        ),
        (b"pass\0word", b"sa\0lt", 4096, 16, "56fa6aa75548099dcc37d7f03425e0c3"),
    ];
    for (password, salt, rounds, len, expected) in cases {
        let derived = pbkdf2_bytes("sha1", password, salt, *rounds, *len).expect("RFC 6070 parameters are valid");
        assert_eq!(hexed(&derived), *expected, "RFC 6070, c={rounds}, dkLen={len}");
    }
}

/// The `digest` argument is HONOURED — the exact defect `NODE_COMPATIBLE.md`
/// measured, where `sha1` and `sha256` answered byte-identical output.
///
/// The SHA-256 expectation is the value that document itself recorded for
/// `pbkdf2Sync("password","salt",1,20,"sha256")` (`120fb6cf…a86548c9`), which is
/// the widely published PBKDF2-HMAC-SHA256 vector for those inputs truncated to
/// 20 bytes. It is quoted from the measurement rather than produced here: this
/// test would be worthless if the number came from the code it checks.
#[test]
fn pbkdf2_digest_argument_changes_the_answer() {
    let sha1 = pbkdf2_bytes("sha1", b"password", b"salt", 1, 20).expect("valid");
    let sha256 = pbkdf2_bytes("sha256", b"password", b"salt", 1, 20).expect("valid");
    assert_eq!(hexed(&sha1), "0c60c80f961f0e71f3a9b524af6012062fe037a6");
    assert_eq!(hexed(&sha256), "120fb6cffcf8b32c43e7225256c4f837a86548c9");
    assert_ne!(sha1, sha256, "the two digests must not answer the same bytes");
}

/// Every name [`super::derive::DIGESTS`] lists derives, and every name it does
/// not is refused rather than answered.
///
/// The refusal half is the one that matters: a name the dispatch does not know
/// used to fall through to a zero-filled buffer.
#[test]
fn pbkdf2_refuses_a_digest_it_does_not_know() {
    for name in super::digests() {
        let derived = pbkdf2_bytes(name, b"p", b"s", 1, 16);
        assert!(derived.is_ok(), "{name} is listed and must derive");
        assert_eq!(derived.expect("just checked").len(), 16);
    }
    for name in ["sha3-256", "blake2b512", "sha512-256", "", "SHA-256", "whirlpool"] {
        assert!(pbkdf2_bytes(name, b"p", b"s", 1, 16).is_err(), "{name} must be refused, not defaulted");
    }
}

/// Case-insensitive, as Node is: `'SHA256'` and `'sha256'` are one algorithm.
#[test]
fn pbkdf2_digest_name_is_case_insensitive() {
    let lower = pbkdf2_bytes("sha256", b"password", b"salt", 1, 20).expect("valid");
    let upper = pbkdf2_bytes("SHA256", b"password", b"salt", 1, 20).expect("valid");
    assert_eq!(lower, upper);
}

/// Zero iterations is refused, not clamped. Node raises `ERR_OUT_OF_RANGE`, and
/// the value `pbkdf2_hmac` answers for it is the unstretched HMAC — a plausible
/// non-key.
#[test]
fn pbkdf2_refuses_zero_iterations() {
    assert!(pbkdf2_bytes("sha256", b"password", b"salt", 0, 20).is_err());
}

// ---------------------------------------------------------------- scrypt ---

/// RFC 7914 §12, vector 1 — the empty-password, empty-salt case at
/// `N = 16, r = 1, p = 1, dkLen = 64`.
#[test]
fn scrypt_matches_rfc_7914_vector_one() {
    let derived = scrypt_bytes(b"", b"", 16, 1, 1, 32 * 1024 * 1024, 64).expect("valid parameters");
    let expected = hex(
        "77d6576238657b203b19ca42c18a0497f16b4844e3074ae8dfdffa3fede21442
         fcd0069ded0948f8326a753a0fc81f17e8d3e0fb2e0d3628cf35e20c38d18906",
    );
    assert_eq!(derived, expected);
}

/// RFC 7914 §12, vector 2 — `N = 1024, r = 8, p = 16, dkLen = 64`.
///
/// This is the vector that pins the cost parameters as LIVE.
/// `NODE_COMPATIBLE.md` measured `{N:16,r:1,p:1}` and `{N:1024,r:8,p:16}`
/// producing identical bytes; vector 1 alone would still pass against an
/// implementation that ignored all three.
#[test]
fn scrypt_matches_rfc_7914_vector_two() {
    let derived = scrypt_bytes(b"password", b"NaCl", 1024, 8, 16, 32 * 1024 * 1024, 64).expect("valid parameters");
    let expected = hex(
        "fdbabe1c9d3472007856e7190d01e9fe7c6ad7cbc8237830e77376634b373162
         2eaf30d92e22a3886ff109279d9830dac727afb94a83ee6d8360cbdfa2cc0640",
    );
    assert_eq!(derived, expected);
    let other = scrypt_bytes(b"password", b"NaCl", 16, 1, 1, 32 * 1024 * 1024, 64).expect("valid parameters");
    assert_ne!(derived, other, "N/r/p must change the answer");
}

/// `N` must be a power of two greater than one — RFC 7914 §2 and Node both.
///
/// `1000` is the case the previous implementation got wrong in the quiet
/// direction: it took `log2().round()`, accepted the value, and derived at
/// `N = 1024`. A key nobody asked for, with no error naming it.
#[test]
fn scrypt_refuses_a_cost_that_is_not_a_power_of_two() {
    for n in [0u64, 1, 3, 1000, 16383] {
        assert!(scrypt_bytes(b"p", b"s", n, 8, 1, 32 * 1024 * 1024, 32).is_err(), "N={n} must be refused");
    }
    assert!(scrypt_bytes(b"p", b"s", 1024, 8, 1, 32 * 1024 * 1024, 32).is_ok());
}

/// `maxmem` is a ceiling that is CHECKED, and checked before the memory is
/// reserved. `128 * 1024 * 8` is 1 MiB, so a 512 KiB ceiling must refuse it and
/// the 32 MiB default must not.
///
/// `N = 1024` rather than Node's default `16384`: the arithmetic under test is
/// the comparison, not the cost, and `cargo test` here is a **debug** build
/// where the workspace pins `opt-level = 0` for every package. A test that pays
/// four times the scrypt work to check a `>` is the iteration-speed rule being
/// broken inside the test suite.
#[test]
fn scrypt_honours_maxmem() {
    assert!(scrypt_bytes(b"p", b"s", 1024, 8, 1, 512 * 1024, 32).is_err());
    assert!(scrypt_bytes(b"p", b"s", 1024, 8, 1, 32 * 1024 * 1024, 32).is_ok());
}

/// `r` and `p` of zero are refused rather than clamped to one.
#[test]
fn scrypt_refuses_a_zero_block_size_or_parallelisation() {
    assert!(scrypt_bytes(b"p", b"s", 16, 0, 1, 32 * 1024 * 1024, 32).is_err());
    assert!(scrypt_bytes(b"p", b"s", 16, 1, 0, 32 * 1024 * 1024, 32).is_err());
}

/// Node's defaults are Node's: `N = 16384, r = 8, p = 1`, `maxmem = 32 MiB` —
/// asserted as the constants the argument reader applies, so a change to one
/// has to change this line too.
#[test]
fn scrypt_defaults_are_nodes() {
    use super::derive::{SCRYPT_DEFAULT_MAXMEM, SCRYPT_DEFAULT_N, SCRYPT_DEFAULT_P, SCRYPT_DEFAULT_R};
    assert_eq!((SCRYPT_DEFAULT_N, SCRYPT_DEFAULT_R, SCRYPT_DEFAULT_P), (16384, 8, 1));
    assert_eq!(SCRYPT_DEFAULT_MAXMEM, 32 * 1024 * 1024);
    // The defaults must be usable against each other: `128 * 16384 * 8` is
    // 16 MiB, inside the 32 MiB ceiling. A default set that refuses itself —
    // `scryptSync(pw, salt, 32)` with no options throwing — is the bug this
    // pins, and it is the COMPARISON that would be wrong, so the comparison is
    // what is asserted. Deriving at the default cost to learn it costs seconds
    // in a debug build and tells us nothing the RFC vectors do not.
    let required = 128u128 * u128::from(SCRYPT_DEFAULT_N) * u128::from(SCRYPT_DEFAULT_R);
    assert!(required <= u128::from(SCRYPT_DEFAULT_MAXMEM), "the default cost must fit the default ceiling");
}

// ------------------------------------------------------------------ HKDF ---

/// RFC 5869 §A.1 — basic test case with SHA-256.
#[test]
fn hkdf_matches_rfc_5869_case_one() {
    let derived = hkdf_bytes(
        "sha256",
        &hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
        &hex("000102030405060708090a0b0c"),
        &hex("f0f1f2f3f4f5f6f7f8f9"),
        42,
    )
    .expect("valid");
    let expected = hex(
        "3cb25f25faacd57a90434f64d0362f2a
         2d2d0a90cf1a5a4c5db02d56ecc4c5bf
         34007208d5b887185865",
    );
    assert_eq!(derived, expected);
}

/// RFC 5869 §A.3 — SHA-256 with zero-length salt and info.
///
/// This is what pins the "empty salt is `Some(&[])`, not `None`" choice in
/// [`super::derive::hkdf_bytes`]: HMAC zero-pads a short key, so an empty key
/// and RFC 5869's HashLen zeros are one extraction. The comment claims it; this
/// asserts it.
#[test]
fn hkdf_matches_rfc_5869_case_three() {
    let derived = hkdf_bytes("sha256", &hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"), b"", b"", 42)
        .expect("valid");
    let expected = hex(
        "8da4e775a563c18f715f802a063c5a31
         b8a11f5c5ee1879ec3454e5f3c738d2d
         9d201395faa4b61a96c8",
    );
    assert_eq!(derived, expected);
}

/// RFC 5869 §A.4 — basic test case with SHA-1. The `digest` argument is live
/// here for the same reason it is for PBKDF2.
#[test]
fn hkdf_matches_rfc_5869_case_four() {
    let ikm = hex("0b0b0b0b0b0b0b0b0b0b0b");
    let salt = hex("000102030405060708090a0b0c");
    let info = hex("f0f1f2f3f4f5f6f7f8f9");
    let derived = hkdf_bytes("sha1", &ikm, &salt, &info, 42).expect("valid");
    let expected = hex(
        "085a01ea1b10f36933068b56efa5ad81
         a4f14b822f5b091568a9cdd4f155fda2
         c22e422478d305f3f896",
    );
    assert_eq!(derived, expected);
    let sha256 = hkdf_bytes("sha256", &ikm, &salt, &info, 42).expect("valid");
    assert_ne!(derived, sha256, "the two digests must not answer the same bytes");
}

/// HKDF answers a NON-EMPTY key — `NODE_COMPATIBLE.md`'s measured defect was
/// zero bytes, which every length-agnostic assertion would have passed.
#[test]
fn hkdf_answers_the_requested_length() {
    for len in [1usize, 16, 32, 42, 255 * 32] {
        let derived = hkdf_bytes("sha256", b"ikm", b"salt", b"info", len).expect("inside 255 * HashLen");
        assert_eq!(derived.len(), len);
        assert!(derived.iter().any(|byte| *byte != 0), "a derived key of all zeros is the defect this pins");
    }
}

/// Past `255 * HashLen` (RFC 5869 §2.3) HKDF has nothing to answer, so it
/// refuses. It used to answer that many zeros.
#[test]
fn hkdf_refuses_a_length_it_cannot_expand_to() {
    assert!(hkdf_bytes("sha256", b"ikm", b"salt", b"info", 255 * 32 + 1).is_err());
    assert!(hkdf_bytes("sha1", b"ikm", b"salt", b"info", 255 * 20 + 1).is_err());
}

/// An unknown digest is refused, not defaulted — the same list, the same rule.
#[test]
fn hkdf_refuses_a_digest_it_does_not_know() {
    for name in super::digests() {
        assert!(hkdf_bytes(name, b"ikm", b"salt", b"info", 16).is_ok(), "{name} is listed and must derive");
    }
    for name in ["sha3-256", "blake2b512", "", "sha-256"] {
        assert!(hkdf_bytes(name, b"ikm", b"salt", b"info", 16).is_err(), "{name} must be refused");
    }
}
