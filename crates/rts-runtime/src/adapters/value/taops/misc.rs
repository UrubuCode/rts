//! Two small runtime trampolines that have no better home: the `BigInt.asIntN`/
//! `asUintN` N-bit wraps (they are the same modulo-2^N arithmetic the typed
//! array element wrap uses) and the uniform-ABI `...rest` packer.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::super::PolyValue;
use super::super::genops;

/// `BigInt.asIntN(bits, v)` — wrap `v` into an N-bit SIGNED integer (the i64
/// interim BigInt model, #219). `asUintN` is the unsigned form.
#[rtse::abi]
pub fn rtsadp_bigint_as_intn(bits_word: u64, val_word: u64) -> u64 {
    let bits = genops::to_number(PolyValue::from_raw(bits_word)) as u32;
    let v = genops::to_number(PolyValue::from_raw(val_word)).trunc() as i64;
    if bits == 0 || bits >= 64 {
        return PolyValue::from_f64(v as f64).raw();
    }
    let m = (v as u64) & (u64::MAX >> (64 - bits));
    let shift = 64 - bits;
    let out = ((m << shift) as i64) >> shift;
    PolyValue::from_f64(out as f64).raw()
}

/// `BigInt.asUintN(bits, v)` — unsigned N-bit wrap.
#[rtse::abi]
pub fn rtsadp_bigint_as_uintn(bits_word: u64, val_word: u64) -> u64 {
    let bits = genops::to_number(PolyValue::from_raw(bits_word)) as u32;
    let v = genops::to_number(PolyValue::from_raw(val_word)).trunc() as i64;
    if bits == 0 || bits >= 64 {
        return PolyValue::from_f64(v as f64).raw();
    }
    let m = (v as u64) & (u64::MAX >> (64 - bits));
    PolyValue::from_f64(m as f64).raw()
}

/// Pack a `...rest` param's value from the uniform-ABI slots: the positional
/// slots `a[start_idx..4]` (TRAILING `undefined`s trimmed — the ABI carries no
/// argc, so an explicit trailing `undefined` argument is indistinguishable
/// from an absent one; documented divergence) followed by every element of the
/// overflow `rest_word` array. Returns a fresh array word.
#[rtse::abi]
pub fn rtsadp_pack_rest(
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    rest_word: u64,
    start_idx: i64,
) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let undef = PolyValue::undefined().raw();
    let slots = [a0, a1, a2, a3];
    let start = start_idx.clamp(0, 4) as usize;
    // Trim trailing undefineds from the positional window.
    let mut end = 4usize;
    while end > start && slots[end - 1] == undef {
        end -= 1;
    }
    for &w in &slots[start..end] {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w as i64);
    }
    let rv = PolyValue::from_raw(rest_word);
    if rv.is_object() {
        let rh = rt_handles::__rtsn_poly_to_handle(rv.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(rh).max(0);
        for i in 0..len {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(rh, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w);
        }
    }
    PolyValue::from_object_handle(rt_handles::__rtsn_poly_from_handle(out)).raw()
}
