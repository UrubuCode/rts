//! Info estatico do processo corrente.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_PID() -> i64 {
    std::process::id() as i64
}

/// Alias para env::args_count — segue o padrao Node/Deno onde
/// argumentos sao expostos via process.argv. Implementacao compartilha
/// o mesmo std::env::args interno, so o simbolo exportado difere.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_ARGS_COUNT() -> i64 {
    std::env::args().count() as i64
}

/// Alias para env::arg_at. Retorna string handle ou 0 out of range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROCESS_ARG_AT(index: i64) -> u64 {
    unsafe extern "C" {
        fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    }
    if index < 0 {
        return 0;
    }
    match std::env::args().nth(index as usize) {
        Some(arg) => unsafe { __RTS_FN_NS_GC_STRING_NEW(arg.as_ptr(), arg.len() as i64) },
        None => 0,
    }
}
