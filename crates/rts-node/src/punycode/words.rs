//! node:punycode — GC interning, the `RangeError` throw bridge, and the value
//! helpers for the `ucs2` sub-object: building callable `Entry::Function` values
//! (word-ABI native fns invoked via the legacy arity-1 path) and decoding
//! number arrays / string words for `ucs2.encode`/`ucs2.decode`.

use rts_engine::heap::handles::{alloc_entry, read_string_handle, Entry, FunctionData};
use rts_engine::heap::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_SHIFT,
    POLY_TAG_STR,
};
use rts_engine::heap::shapes::{handle_word_auto, string_word};

use rts_engine::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// Intern a Rust string as a GC string handle (an ABI `Handle`/`StrPtr` source).
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Throw a JS `kind` Error (paired with `MemberFlags::THROWS` on the member).
pub fn throw(kind: &str, msg: &str) {
    unsafe {
        __rtsadp_throw_js_error(kind.as_ptr(), kind.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

/// A string element word.
pub fn str_word(s: &str) -> i64 {
    string_word(s.as_bytes()) as i64
}

/// A number element word (a genuine inline `f64`).
pub fn num_word(v: f64) -> i64 {
    v.to_bits() as i64
}

/// An array value word from element words.
pub fn array_word(words: Vec<i64>) -> i64 {
    handle_word_auto(alloc_entry(Entry::Vec(Box::new(words)))) as i64
}

/// Build a callable `Entry::Function` value word wrapping the word-ABI native
/// `fp` (`extern "C" fn(i64) -> i64`, invoked via the legacy arity-1 path).
pub fn fn_value(fp: *const u8, name: &str, arity: u8) -> i64 {
    let data = FunctionData {
        fn_ptr: fp as u64,
        arity,
        name: name.into(),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        // Uniform-thunk ABI (`(env, a0..a3, rest) -> word`): the invoker passes
        // and returns raw PolyValue WORDS with no reboxing — required so the
        // array/string result is not re-wrapped as a plain number.
        uniform_thunk: true,
    };
    handle_word_auto(alloc_entry(Entry::Function(Box::new(data)))) as i64
}

/// Decode a value word that must be a string → its text, or `None`.
pub fn word_string(w: u64) -> Option<String> {
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE || ((w >> POLY_TAG_SHIFT) & POLY_TAG_MASK) != POLY_TAG_STR
    {
        return None;
    }
    let h = poly_handle_normalize(w)?;
    read_string_handle(h)
}

/// Decode an array value word into its element f64 values (for `ucs2.encode`).
pub fn word_number_array(w: u64) -> Option<Vec<f64>> {
    use rts_engine::heap::handles::with_entry;
    let h = poly_handle_normalize(w)?;
    with_entry(h, |e| match e {
        Some(Entry::Vec(v)) => Some(v.iter().map(|&e| word_to_f64(e as u64)).collect()),
        _ => None,
    })
}

/// A value word → its numeric value (inline double, or boxed INT32).
fn word_to_f64(w: u64) -> f64 {
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        return f64::from_bits(w);
    }
    // Boxed INT32: low 32 payload bits, sign-extended.
    (w & POLY_PAYLOAD_MASK) as u32 as i32 as f64
}
