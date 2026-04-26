//! `mem` runtime: layout constants + swap/drop/forget primitives.

// Constants exposed as () -> i64 functions (see math::consts pattern).

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_I64() -> i64 {
    std::mem::size_of::<i64>() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_F64() -> i64 {
    std::mem::size_of::<f64>() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_I32() -> i64 {
    std::mem::size_of::<i32>() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SIZE_OF_BOOL() -> i64 {
    std::mem::size_of::<bool>() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_ALIGN_OF_I64() -> i64 {
    std::mem::align_of::<i64>() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_ALIGN_OF_F64() -> i64 {
    std::mem::align_of::<f64>() as i64
}

/// Idiom: caller usa `let tmp = a; a = mem.swap_i64(a, b); b = tmp;`
/// — RTS sem refs nao oferece swap-by-pointer real, so retorna `b`
/// para enfatizar a operacao (caller faz a atribuicao).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_SWAP_I64(_a: i64, b: i64) -> i64 {
    b
}

/// Forca free de um handle GC (string, vec, map, regex, ...).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_DROP_HANDLE(handle: u64) {
    let _ = super::super::gc::handles::table().lock().unwrap().free(handle);
}

/// Esquece handle (no-op em GC slab — vaza ate program exit).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_FORGET_HANDLE(_handle: u64) {
    // Intencional: nao chama free.
}

/// `mem.replace_i64(slot, new)` — caller escreve `new` no slot e usa
/// retorno como valor antigo. Como nao temos refs, esta funcao
/// apenas retorna `slot` (caller faz a atribuicao do new_val).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MEM_REPLACE_I64(slot: i64, _new_val: i64) -> i64 {
    slot
}
