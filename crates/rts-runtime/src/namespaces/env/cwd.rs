//! Leitura e modificacao do diretorio de trabalho corrente.

use std::env;
use std::path::Path;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Retorna handle de string com o cwd, ou 0 em falha (ex: diretorio
/// removido enquanto o processo roda).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_CWD() -> u64 {
    match env::current_dir() {
        Ok(path) => {
            let s = path.to_string_lossy();
            unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
        }
        Err(_) => 0,
    }
}

/// Muda o cwd. Retorna 0 em sucesso, -1 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_SET_CWD(path_ptr: *const u8, path_len: i64) -> i64 {
    if path_ptr.is_null() || path_len < 0 {
        return -1;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(path_ptr, path_len as usize) };
    let Ok(path) = std::str::from_utf8(slice) else {
        return -1;
    };
    match env::set_current_dir(Path::new(path)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
