//! Hash operations backed by std::hash::DefaultHasher (SipHash-1-3).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_slice(bytes: &[u8]) -> i64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_STR(ptr: *const u8, len: i64) -> i64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    // SAFETY: caller contract — UTF-8 valido cobrindo `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    hash_slice(slice)
}

/// Hash de um bloco de bytes arbitrarios. `ptr` pode ser qualquer
/// endereco valido (ex: `buffer.ptr(handle)`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_BYTES(ptr: i64, len: i64) -> i64 {
    if ptr == 0 || len < 0 {
        return 0;
    }
    // SAFETY: caller passou um ponteiro valido vindo de buffer/gc.
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    hash_slice(slice)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_I64(value: i64) -> i64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish() as i64
}

/// Combina dois hashes em um novo. Usa mistura estilo boost::hash_combine —
/// preserva entropia dos dois operandos sem ser comutativo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HASH_HASH_COMBINE(h1: i64, h2: i64) -> i64 {
    // Constante = golden ratio truncada. Aritmetica bitwise em u64
    // pra preservar os bits altos; shift << 6 em i64 sinalizado seria
    // ambiguo em valores negativos.
    let (a, b) = (h1 as u64, h2 as u64);
    let combined = a ^ b
        .wrapping_add(0x517c_c1b7_2722_0a95)
        .wrapping_add(a << 6)
        .wrapping_add(a >> 2);
    combined as i64
}
