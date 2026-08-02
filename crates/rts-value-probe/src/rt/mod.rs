//! The `extern "C"` trampolines the JITted kernels call — replicas of the real
//! ones, so the call profile under test is the real call profile.
//!
//! - [`probe_vec_get_locked`] ≙ `__rtsn_vec_get_by_payload`
//!   (`rts-natives/src/heap/payload_ops.rs`): one shard `Mutex` per field touch.
//! - [`probe_vec_get_unlocked`] — same body, `Mutex` removed.
//! - [`probe_adp_mul`] / [`probe_adp_add`] ≙ `__rtsadp_mul` / `__rtsadp_add`
//!   (`rts-runtime/src/adapters/value/genops_arith.rs::arith`): `to_number` twice,
//!   operate in `f64`, `number_result` to re-tighten to a tagged int32 when exact.
//!
//! All are `#[inline(never)]` `extern "C"`: the JIT resolves them through
//! `JITBuilder::symbol`, so they are opaque calls to Cranelift exactly as the
//! real ones are — an optimization barrier, not something the egraph sees into.

pub mod dict;
pub mod ops;
pub mod stdshape;
pub mod strings;
pub mod sym;
pub mod values;
pub mod writes;

use crate::poly;
use crate::slab;

// --- field read ------------------------------------------------------------

/// `__rtsn_vec_get_by_payload`: shard-route the 48-bit payload, take the shard
/// lock, index the slab, deref the `Box<Vec<i64>>`, bounds-check, return.
#[inline(never)]
pub extern "C" fn probe_vec_get_locked(payload: i64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    slab::sharded::vec_get(payload as u64, index)
}

/// Byte-identical minus the `Mutex`. The delta against
/// [`probe_vec_get_locked`] is the lock's contribution and nothing else.
#[inline(never)]
pub extern "C" fn probe_vec_get_unlocked(payload: i64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    slab::unlocked::vec_get(payload as u64, index)
}

// --- generic arithmetic ----------------------------------------------------

/// `genops_arith::arith(a, b, f64::mul)` + `number_result`.
#[inline(never)]
pub extern "C" fn probe_adp_mul(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x * y) as i64
}

/// `genops::add`'s numeric arm (the probe's kernels never produce strings, so
/// the string-concat path — 3+ locked heap ops — is deliberately out of scope).
#[inline(never)]
pub extern "C" fn probe_adp_add(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x + y) as i64
}

// --- allocation ------------------------------------------------------------

/// One object `{x, y}` in the sharded mutex slab: `Box<Vec<i64>>` + slot push
/// under the shard lock. Returns the 48-bit payload.
#[inline(never)]
pub extern "C" fn probe_alloc_locked(x: i64, y: i64, shape: i64) -> i64 {
    slab::sharded::alloc_object(&[x, y], shape) as i64
}

/// Same object, bump-allocated inline in the flat arena. Returns the element
/// offset (the pointer-compression payload).
#[inline(never)]
pub extern "C" fn probe_alloc_bump(x: i64, y: i64, shape: i64) -> i64 {
    slab::arena::alloc_object(&[x, y], shape) as i64
}

/// Every symbol the JIT kernels may import, in one place so
/// `emit::make_module` and any future kernel stay in sync.
pub fn symbols() -> Vec<(&'static str, *const u8)> {
    let mut v = base_symbols();
    v.extend(ops::symbols());
    v.extend(writes::symbols());
    v
}

fn base_symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        ("probe_vec_get_locked", probe_vec_get_locked as *const u8),
        (
            "probe_vec_get_unlocked",
            probe_vec_get_unlocked as *const u8,
        ),
        ("probe_adp_mul", probe_adp_mul as *const u8),
        ("probe_adp_add", probe_adp_add as *const u8),
        ("probe_alloc_locked", probe_alloc_locked as *const u8),
        ("probe_alloc_bump", probe_alloc_bump as *const u8),
        (
            "probe_vec_set_locked",
            probe_vec_set_locked as *const u8,
        ),
        ("probe_inline_get", probe_inline_get as *const u8),
        ("probe_to_number", values::probe_to_number as *const u8),
        ("probe_dict_get", dict::probe_dict_get as *const u8),
        (
            "probe_dict_get_borrowed",
            dict::probe_dict_get_borrowed as *const u8,
        ),
        ("probe_to_boolean", values::probe_to_boolean as *const u8),
        ("probe_adp_iadd", values::probe_adp_iadd as *const u8),
        ("probe_err_pending", values::probe_err_pending as *const u8),
        ("probe_call_uniform", values::probe_call_uniform as *const u8),
        ("probe_call_native_f64", values::probe_call_native_f64 as *const u8),
        ("probe_vec_push_locked", values::probe_vec_push_locked as *const u8),
        ("probe_obj_set_proto", values::probe_obj_set_proto as *const u8),
        ("probe_gc_tick", values::probe_gc_tick as *const u8),
        ("probe_str_len", stdshape::probe_str_len as *const u8),
        ("probe_char_str", stdshape::probe_char_str as *const u8),
        ("probe_str_eq_lit", stdshape::probe_str_eq_lit as *const u8),
        ("probe_scan_native", stdshape::probe_scan_native as *const u8),
        ("probe_chunk_add", sym::probe_chunk_add as *const u8),
        ("probe_chunk_add_raw", sym::probe_chunk_add_raw as *const u8),
    ]
}

/// The inline-slot slab read, behind the SAME opaque `extern "C"` boundary the
/// locked/unlocked ones use. The delta against [`probe_vec_get_unlocked`] is the
/// `Box<Vec<i64>>` indirection alone; the delta against the pure-IR variant is
/// the CALL alone.
#[inline(never)]
pub extern "C" fn probe_inline_get(payload: i64, index: i64) -> i64 {
    slab::blocks::inline_slab::get(payload as u64, index)
}

/// `__RTS_FN_NS_COLLECTIONS_VEC_SET` — the write half of array element access.
#[inline(never)]
pub extern "C" fn probe_vec_set_locked(payload: i64, index: i64, value: i64) -> i64 {
    slab::sharded::vec_set(payload as u64, index, value);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_arith_matches_the_re_tightening_contract() {
        // 6 * 7 = 42, exactly representable → comes back as a TAGGED INT32, not
        // a double. That round-trip is why the next iteration's `to_number`
        // takes the int path — the real trampolines behave the same way.
        let a = poly::from_f64(6.0);
        let b = poly::from_f64(7.0);
        let r = probe_adp_mul(a as i64, b as i64) as u64;
        assert!(poly::is_boxed(r));
        assert_eq!(poly::to_number(r), 42.0);

        // Non-integral stays an inline double.
        let c = poly::from_f64(1.5);
        let r2 = probe_adp_mul(c as i64, b as i64) as u64;
        assert!(!poly::is_boxed(r2));
        assert_eq!(poly::as_f64(r2), 10.5);
    }

    #[test]
    fn locked_and_unlocked_reads_agree() {
        let p = probe_alloc_locked(poly::from_f64(3.0) as i64, poly::from_f64(4.0) as i64, 7);
        assert_eq!(probe_vec_get_locked(p, 0), 7);
        assert_eq!(
            poly::as_f64(probe_vec_get_locked(p, 1) as u64),
            3.0
        );
    }
}
