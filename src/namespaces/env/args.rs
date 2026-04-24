//! Acesso aos argumentos de linha de comando do processo.

use std::env;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_ARGS_COUNT() -> i64 {
    env::args().count() as i64
}

/// Retorna handle de string com o argumento em `index`, ou 0 se o indice
/// estiver fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_ARG_AT(index: i64) -> u64 {
    if index < 0 {
        return 0;
    }
    let Some(arg) = env::args().nth(index as usize) else {
        return 0;
    };
    unsafe { __RTS_FN_NS_GC_STRING_NEW(arg.as_ptr(), arg.len() as i64) }
}
