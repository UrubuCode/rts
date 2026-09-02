//! `randomBytes`/`randomFillSync`/`randomInt`/`randomUUID`/`getRandomValues`
//! — over §2.2 "Randomness" and §4's security note: every draw here goes
//! through `getrandom` (OS CSPRNG), never the engine's `Math.random` xorshift
//! generator, per that note's explicit requirement.
//!
//! # Not implemented, by name
//!
//! The callback forms of `randomBytes`/`randomFill`/`randomInt` — offloading
//! to the shared tokio runtime is `§5.7`'s FLAGGED gap (the runtime currently
//! lives in `rts-std`, which this crate must not depend on); only the sync
//! forms are implemented. `getRandomValues`'s "integer typed arrays only"
//! and 65 536-byte-cap checks are NOT enforced — this module has no reach
//! into a typed array's element kind (see `entry::bytes_of`'s doc: it
//! answers a byte window, not a kind), so a caller passing a `Float64Array`
//! is filled with random bytes rather than refused.

use rts_core::entry;

use super::util;

pub(crate) fn os_bytes(len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    // `getrandom` 0.4 — vetted in `docs/reference/node/crates.md` §4.1:
    // "getrandom | 0.4 | MIT/Apache | randomBytes/randomFillSync/
    // getRandomValues". A draw that fails (exhausted OS entropy source) is
    // left as all-zero bytes rather than raised — Node's own failure mode
    // here is treated as unreachable on a sane host, and this is a fallback
    // for an unreachable case rather than a validation the caller can hit.
    let _ = getrandom::fill(&mut out);
    out
}

/// `crypto.randomBytes(size)` — answers a `Uint8Array` (see `hash.rs`'s
/// module doc for why not a `Buffer` instance). A negative or non-finite
/// `size` raises a catchable error, matching Node's `ERR_OUT_OF_RANGE` in
/// spirit (see `throw_type_error`'s doc for why the class raised here is
/// `TypeError` rather than `RangeError`).
pub(super) extern "C" fn random_bytes(_e: u64, _this: u64, size: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let negative = entry::with_runtime(|context| util::integer(context, size).map(|value| value < 0).unwrap_or(false));
    if negative {
        entry::throw_type_error("The value of \"size\" is out of range. It must be >= 0.");
        return entry::undefined_value();
    }
    entry::with_runtime(|context| {
        let len = util::integer(context, size).filter(|value| *value >= 0).unwrap_or(0) as usize;
        let bytes = os_bytes(len);
        entry::make_buffer(context, &bytes)
    })
}

/// `crypto.randomFillSync(buffer, offset?, size?)`. Fills `buffer`'s own
/// window in place (through [`entry::write_bytes`], the same primitive
/// `fs`'s `readSync` writes through) and answers `buffer` back, matching
/// Node's `=> T` signature.
pub(super) extern "C" fn random_fill_sync(
    _e: u64,
    _this: u64,
    buffer: u64,
    offset: u64,
    size: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        let offset = util::integer(context, offset).filter(|value| *value >= 0).unwrap_or(0) as usize;
        let requested = util::integer(context, size).filter(|value| *value >= 0).map(|value| value as usize);
        let full = entry::bytes_of(context, buffer).map(|bytes| bytes.len()).unwrap_or(0);
        let len = requested.unwrap_or(full.saturating_sub(offset));
        let bytes = os_bytes(len);
        entry::write_bytes(context, buffer, offset, &bytes);
        buffer
    })
}

/// `crypto.getRandomValues(typedArray)` — fills in place and answers the
/// same reference, the Web Crypto shape. See this module's "Not
/// implemented" note for the two checks real Node also does that this does
/// not.
pub(super) extern "C" fn get_random_values(_e: u64, _this: u64, typed_array: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let len = entry::bytes_of(context, typed_array).map(|bytes| bytes.len()).unwrap_or(0);
        let bytes = os_bytes(len);
        entry::write_bytes(context, typed_array, 0, &bytes);
        typed_array
    })
}

/// `crypto.randomInt(minOrMax, max?)` — unbiased via rejection sampling
/// (Node's own requirement, §4: "RTS must not naively `% range`, which is
/// biased"). One argument means `min = 0`. `max <= min` raises a catchable
/// error (`ERR_OUT_OF_RANGE` in Node; here a `TypeError` — see `throw_type_error`'s
/// doc for why that is the only class this crate can raise with).
pub(super) extern "C" fn random_int(_e: u64, _this: u64, min_or_max: u64, max: u64, _a2: u64, _a3: u64) -> u64 {
    let bad = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let a = util::integer(context, min_or_max).unwrap_or(0);
        let (min, max) = if max == absent { (0, a) } else { (a, util::integer(context, max).unwrap_or(a)) };
        if max <= min { Some(()) } else { None }
    });
    if bad.is_some() {
        entry::throw_type_error("The value of \"max\" is out of range. It must be greater than the value of \"min\".");
        return entry::undefined_value();
    }
    entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let a = util::integer(context, min_or_max).unwrap_or(0);
        let (min, max) = if max == absent { (0, a) } else { (a, util::integer(context, max).unwrap_or(a)) };
        let range = (max - min) as u64;
        // Rejection sampling: draw `u64`s, discard the ones that would bias
        // the modulo toward the low end, exactly as §4 requires.
        let limit = u64::MAX - (u64::MAX % range);
        let mut buf = [0u8; 8];
        let value = loop {
            let _ = getrandom::fill(&mut buf);
            let drawn = u64::from_le_bytes(buf);
            if drawn < limit {
                break drawn % range;
            }
        };
        entry::make_number((min + value as i64) as f64)
    })
}

/// `crypto.randomUUID()` — RFC 4122 v4, hand-rolled (16 CSPRNG bytes with
/// the version/variant bits set) rather than pulling the `uuid` crate,
/// per `docs/reference/node/crates.md` §4.1's own note that this is the
/// zero-dep option. `options.disableEntropyCache` has no effect — this
/// module keeps no entropy cache to disable.
pub(super) extern "C" fn random_uuid(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let joined = hex.join("");
    let text = format!(
        "{}-{}-{}-{}-{}",
        &joined[0..8],
        &joined[8..12],
        &joined[12..16],
        &joined[16..20],
        &joined[20..32]
    );
    entry::with_runtime(|context| entry::make_string(context, &text))
}

/// `crypto.timingSafeEqual(a, b)`. Node throws on a length mismatch
/// (`ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH`, §4); this raises a catchable
/// `TypeError` for the same case, per `throw_type_error`'s doc.
///
/// Both sides go through [`util::binary_bytes`], not `entry::bytes_of`. That
/// answers only for a value that OWNS bytes, so two DIFFERENT strings both
/// read as empty and compared EQUAL — a security predicate answering true for
/// unequal inputs, which is the worst direction this particular function can
/// be wrong in. Node itself refuses a string here rather than coercing it;
/// coercing is the divergence, and it is the one that cannot report a false
/// match.
pub(super) extern "C" fn timing_safe_equal(_e: u64, _this: u64, a: u64, b: u64, _a2: u64, _a3: u64) -> u64 {
    let mismatched = entry::with_runtime(|context| {
        let left = util::binary_bytes(context, a);
        let right = util::binary_bytes(context, b);
        left.len() != right.len()
    });
    if mismatched {
        entry::throw_type_error("Input buffers must have the same byte length");
        return entry::undefined_value();
    }
    entry::with_runtime(|context| {
        let left = util::binary_bytes(context, a);
        let right = util::binary_bytes(context, b);
        let mut diff = 0u8;
        for (l, r) in left.iter().zip(right.iter()) {
            diff |= l ^ r;
        }
        entry::boolean_value(diff == 0)
    })
}
