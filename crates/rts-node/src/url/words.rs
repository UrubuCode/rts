//! node:url — shared helpers: GC interning, object building, and the WHATWG
//! `URL` externs (defined in the runtime layer's `globals/url`, reached by
//! symbol at the final link — the same cross-crate pattern the fs/dns promise
//! modules use). Lets `pathToFileURL` construct a real `URL` and
//! `urlToHttpOptions`/`fileURLToPath` read one.

use rts_engine::heap::handles::{alloc_entry, read_string_handle, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, null_word, string_word};

use rts_engine::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

unsafe extern "C" {
    // WHATWG URL (runtime layer, globals/url). `ptr`/`len` are the i64-cast
    // string parts the URL externs already accept; getters take a URL handle.
    fn __rtsm_global_url_new(ptr: i64, len: i64) -> u64;
    fn __rtsm_global_url_href(h: u64) -> u64;
    fn __rtsm_global_url_protocol(h: u64) -> u64;
    fn __rtsm_global_url_hostname(h: u64) -> u64;
    fn __rtsm_global_url_port(h: u64) -> u64;
    fn __rtsm_global_url_pathname(h: u64) -> u64;
    fn __rtsm_global_url_search(h: u64) -> u64;
    fn __rtsm_global_url_hash(h: u64) -> u64;
    fn __rtsm_global_url_username(h: u64) -> u64;
    fn __rtsm_global_url_password(h: u64) -> u64;
}

/// Intern a Rust string as a GC string handle.
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Construct a real WHATWG `URL` from a string, returning its handle.
pub fn url_new(s: &str) -> u64 {
    unsafe { __rtsm_global_url_new(s.as_ptr() as i64, s.len() as i64) }
}

/// Read a `URL` component (getter extern) as a Rust string.
macro_rules! url_getter {
    ($fn_name:ident, $extern_fn:ident) => {
        pub fn $fn_name(h: u64) -> String {
            let sh = unsafe { $extern_fn(h) };
            read_string_handle(sh).unwrap_or_default()
        }
    };
}
url_getter!(url_href, __rtsm_global_url_href);
url_getter!(url_protocol, __rtsm_global_url_protocol);
url_getter!(url_hostname, __rtsm_global_url_hostname);
url_getter!(url_port, __rtsm_global_url_port);
url_getter!(url_pathname, __rtsm_global_url_pathname);
url_getter!(url_search, __rtsm_global_url_search);
url_getter!(url_hash, __rtsm_global_url_hash);
url_getter!(url_username, __rtsm_global_url_username);
url_getter!(url_password, __rtsm_global_url_password);

/// The href of a `write`-style argument that is EITHER a URL string
/// (`Entry::String`) or a live `URL` instance (any other handle → read its
/// `.href`). Empty on failure.
pub fn arg_href(handle: u64) -> String {
    use rts_engine::heap::handles::with_entry;
    let is_string = with_entry(handle, |e| matches!(e, Some(Entry::String(_))));
    if is_string {
        read_string_handle(handle).unwrap_or_default()
    } else {
        url_href(handle)
    }
}

/// A string object-field word.
pub fn str_word(s: &str) -> i64 {
    string_word(s.as_bytes()) as i64
}

/// A number object-field word.
pub fn num_word(v: f64) -> i64 {
    v.to_bits() as i64
}

/// The `null` field word.
pub fn null_w() -> i64 {
    null_word() as i64
}

/// Build a shaped object → raw handle.
pub fn object(keys: &[&str], values: &[i64]) -> u64 {
    alloc_shaped_object(keys, values)
}

/// Build a `Buffer` (`Entry::Buffer`) from raw bytes → handle.
pub fn buffer(bytes: Vec<u8>) -> u64 {
    alloc_entry(Entry::Buffer(bytes))
}

/// The `(key, value_word)` pairs of a shaped object handle (for `format`
/// reading a urlObject argument), in shape order.
pub fn object_entries(handle: u64) -> Vec<(String, i64)> {
    use rts_engine::heap::handles::with_entry;
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};
    use rts_engine::heap::shapes::global_shape_keys;
    with_entry(handle, |e| {
        let slots = match e {
            Some(Entry::Vec(v)) => v,
            _ => return Vec::new(),
        };
        if slots.is_empty() {
            return Vec::new();
        }
        let w0 = slots[0] as u64;
        if (w0 & POLY_BOX_BASE) != POLY_BOX_BASE {
            return Vec::new();
        }
        let id = (w0 & POLY_PAYLOAD_MASK) as u32;
        match global_shape_keys(id) {
            Some(k) if k.len() + 1 == slots.len() => {
                k.into_iter().zip(slots[1..].iter().copied()).collect()
            }
            _ => Vec::new(),
        }
    })
}

/// A value word decoded to its scalar string form (string handle → text; a
/// number → decimal; `true`/`false` → those words; else `None`).
pub fn word_scalar(w: u64) -> Option<String> {
    use rts_engine::heap::poly::{
        poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_SHIFT,
        POLY_TAG_SINGLETON, POLY_TAG_STR,
    };
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        let n = f64::from_bits(w);
        return Some(if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") });
    }
    let tag = (w >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    if tag == POLY_TAG_STR {
        return poly_handle_normalize(w).and_then(read_string_handle);
    }
    if tag == POLY_TAG_SINGLETON {
        return match w & POLY_PAYLOAD_MASK {
            3 => Some("true".to_string()),
            2 => Some("false".to_string()),
            _ => None,
        };
    }
    // boxed INT32
    Some(format!("{}", (w & POLY_PAYLOAD_MASK) as u32 as i32))
}
