//! Crate-shared VALUE helpers for `rts-node`: reading an ABI string, interning,
//! byte input/output, options-object fields, and decoding a `PolyValue` argument
//! word into a typed [`Val`].
//!
//! Why [`Val`] exists: the Registry resolves an overload by **arity**, not by
//! argument type (`Class::resolve_instance_method` / `Module::member`). Node's
//! own signatures are overloaded by TYPE at the same arity — `dgram.createSocket`
//! takes `(type)` or `(options)`; `socket.bind` takes `(port)`, `(options)` or
//! `(callback)`. Node itself resolves those in JS by inspecting the argument.
//! RTS does the same thing natively: the member declares `AbiType::PolyValue`
//! and the impl inspects the word's tag — polymorphism on the value, never a
//! per-shape guess.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{
    poly_handle_normalize, poly_number, poly_tag, POLY_BOX_BASE, POLY_PAYLOAD_MASK,
    POLY_SING_FALSE, POLY_SING_NULL, POLY_SING_TRUE, POLY_TAG_FUNCTION, POLY_TAG_OBJECT,
    POLY_TAG_SINGLETON, POLY_TAG_STR,
};
use rts_engine::heap::shapes::global_shape_keys;

use rts_engine::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

unsafe extern "C" {
    /// Truthiness of a PolyValue word — JS `ToBoolean`, returning an UNBOXED
    /// 0/1 (not a boxed boolean).
    fn __rtsadp_to_boolean(v: u64) -> u64;
}

/// Read an `AbiType::StrPtr` argument.
pub fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

/// A Rust string → a GC string handle (an `AbiType::Handle`-returned string).
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// A byte slice → a `Uint8Array`-shaped `Entry::Vec` (each byte an inline-f64
/// number word), so JS `.length`/indexing work.
pub fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// A string list → an `Entry::Vec` of string words.
pub fn string_array(items: &[String]) -> u64 {
    use rts_engine::heap::shapes::string_word;
    let words: Vec<i64> = items.iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// Read the bytes behind a HANDLE: `Entry::String` (UTF-8), `Entry::Buffer`, an
/// `Entry::Vec` (`Uint8Array`/`Buffer`) of number words, or an `ArrayBuffer`'s
/// backing store.
pub fn read_bytes(handle: u64) -> Vec<u8> {
    let direct = with_entry(handle, |e| match e {
        Some(Entry::Buffer(b)) => Some(b.clone()),
        Some(Entry::String(s)) => Some(s.clone()),
        Some(Entry::Vec(v)) => Some(v.iter().map(|&w| word_byte(w as u64)).collect()),
        _ => None,
    });
    if let Some(bytes) = direct {
        return bytes;
    }
    // An ArrayBuffer keeps its bytes behind a stable raw pointer, not in a Vec.
    let len = rts_engine::heap::handles::arraybuffer_len(handle);
    let ptr = rts_engine::heap::handles::arraybuffer_ptr(handle);
    if len == 0 || ptr.is_null() {
        return Vec::new();
    }
    // SAFETY: `arraybuffer_ptr` returns a pointer to `len` live bytes of the
    // handle's backing store, valid while the handle lives — and it does, since
    // the caller holds it.
    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

/// One element word of a `Uint8Array`-shaped array → its byte value.
fn word_byte(w: u64) -> u8 {
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        f64::from_bits(w) as u8
    } else {
        (w & POLY_PAYLOAD_MASK) as u32 as u8
    }
}

/// A decoded `PolyValue` argument word.
pub enum Val {
    Undefined,
    Null,
    /// A boolean. Carries no payload: callers that want truthiness read the word
    /// through `opt_bool` (the engine's own `ToBoolean`); what this variant is
    /// for is telling "a boolean was passed" apart from "nothing was passed".
    Bool,
    Num(f64),
    /// A string and its handle.
    Str(String),
    /// An object/array HANDLE (already normalized off the boxed word).
    Obj(u64),
    /// A function HANDLE (already normalized off the boxed word).
    Func(u64),
}

impl Val {
    /// Whether the argument was absent (`undefined`) or explicitly `null` —
    /// Node treats both as "not supplied" for dgram's optional params.
    pub fn is_nullish(&self) -> bool {
        matches!(self, Val::Undefined | Val::Null)
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_func(&self) -> Option<u64> {
        match self {
            Val::Func(h) => Some(*h),
            _ => None,
        }
    }
}

/// Decode a `PolyValue` argument word into a typed [`Val`].
pub fn val(word: u64) -> Val {
    let Some(tag) = poly_tag(word) else {
        return Val::Num(f64::from_bits(word));
    };
    match tag {
        POLY_TAG_SINGLETON => match word & POLY_PAYLOAD_MASK {
            POLY_SING_NULL => Val::Null,
            POLY_SING_TRUE | POLY_SING_FALSE => Val::Bool,
            _ => Val::Undefined,
        },
        POLY_TAG_STR => {
            let h = poly_handle_normalize(word).unwrap_or(word);
            Val::Str(handle_string(h))
        }
        POLY_TAG_FUNCTION => Val::Func(poly_handle_normalize(word).unwrap_or(word)),
        POLY_TAG_OBJECT => Val::Obj(poly_handle_normalize(word).unwrap_or(word)),
        _ => match poly_number(word) {
            Some(n) => Val::Num(n),
            None => Val::Undefined,
        },
    }
}

/// The UTF-8 text behind a string handle.
pub fn handle_string(handle: u64) -> String {
    with_entry(handle, |e| match e {
        Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
        _ => String::new(),
    })
}

/// Read a field WORD off an options object handle. Handles both representations:
/// a shaped object literal (`Entry::Vec`: slot 0 = shape id, the rest the values,
/// keyed by `global_shape_keys`) and an `Entry::Map`.
pub fn opt_word(options: u64, key: &str) -> Option<u64> {
    with_entry(options, |e| match e {
        Some(Entry::Vec(slots)) if !slots.is_empty() => {
            let w0 = slots[0] as u64;
            if (w0 & POLY_BOX_BASE) != POLY_BOX_BASE {
                return None;
            }
            let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
            let keys = global_shape_keys(shape_id)?;
            if keys.len() + 1 != slots.len() {
                return None;
            }
            keys.iter().position(|k| k == key).map(|i| slots[i + 1] as u64)
        }
        Some(Entry::Map(m)) => m.get(key).map(|&w| w as u64),
        _ => None,
    })
}

/// Read a boolean flag off an options object (`{ recursive: true }`). A missing
/// field or a non-object → `false`.
pub fn opt_bool(options: u64, key: &str) -> bool {
    match opt_word(options, key) {
        Some(w) => unsafe { __rtsadp_to_boolean(w) != 0 },
        None => false,
    }
}

/// Read a numeric field off an options object. Missing/non-numeric → `None`.
pub fn opt_num(options: u64, key: &str) -> Option<f64> {
    match val(opt_word(options, key)?) {
        Val::Num(n) => Some(n),
        _ => None,
    }
}

/// Read a string field off an options object. Missing/non-string → `None`.
pub fn opt_str(options: u64, key: &str) -> Option<String> {
    match val(opt_word(options, key)?) {
        Val::Str(s) => Some(s),
        _ => None,
    }
}

/// Whether an options object actually HAS the field (present but `undefined`
/// counts as absent, like Node's `options.x !== undefined` checks).
pub fn opt_has(options: u64, key: &str) -> bool {
    matches!(opt_word(options, key).map(val), Some(v) if !v.is_nullish())
}
