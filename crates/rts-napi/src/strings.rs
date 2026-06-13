//! Strings N-API: `napi_create_string_utf8` + `napi_get_value_string_utf8`
//! (protocolo de duas passagens). Ver docs/specs/napi-implementation.md (Etapa 6).

use std::ffi::c_char;

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value, NAPI_AUTO_LENGTH};

use napi_status::{napi_invalid_arg, napi_ok, napi_string_expected};

/// Cria uma string a partir de UTF-8 do addon. `length == NAPI_AUTO_LENGTH`
/// (= `(size_t)-1`) → mede via `strlen`. A string é copiada para o heap GC.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_utf8(
    env: napi_env,
    str_: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    if str_.is_null() {
        // String vazia para ptr nulo (N-API aceita com length 0).
        let h = alloc_entry(Entry::String(Vec::new()));
        unsafe { crate::scopes::track_in_env(env, h) };
        unsafe { *result = value_from_handle(h) };
        return napi_ok;
    }

    let bytes: Vec<u8> = if length == NAPI_AUTO_LENGTH {
        // strlen: até o primeiro NUL.
        let mut len = 0usize;
        unsafe {
            while *str_.add(len) != 0 {
                len += 1;
            }
        }
        unsafe { std::slice::from_raw_parts(str_ as *const u8, len) }.to_vec()
    } else {
        unsafe { std::slice::from_raw_parts(str_ as *const u8, length) }.to_vec()
    };

    let h = alloc_entry(Entry::String(bytes));
    unsafe { crate::scopes::track_in_env(env, h) };
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

/// Protocolo de duas passagens:
/// - `buf == NULL` → escreve em `*result` o nº de bytes UTF-8 (SEM o NUL) e
///   retorna `napi_ok` (medição).
/// - `buf != NULL` → copia até `bufsize-1` bytes respeitando fronteira de char
///   UTF-8, escreve o NUL terminador, e põe em `*result` o nº de bytes
///   copiados (SEM o NUL).
///
/// `result` pode ser nulo (o addon descarta o comprimento).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_utf8(
    _env: napi_env,
    value: napi_value,
    buf: *mut c_char,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    let h = handle_from_value(value);
    let bytes = match with_entry(h, |e| match e {
        Some(Entry::String(b)) => Some(b.clone()),
        _ => None,
    }) {
        Some(b) => b,
        None => return napi_string_expected,
    };

    // Passagem 1: medição.
    if buf.is_null() {
        if !result.is_null() {
            unsafe { *result = bytes.len() };
        }
        return napi_ok;
    }

    // Passagem 2: cópia. Reserva 1 byte para o NUL.
    if bufsize == 0 {
        if !result.is_null() {
            unsafe { *result = 0 };
        }
        return napi_ok;
    }
    let max = bufsize - 1;
    let copy_len = floor_char_boundary(&bytes, max);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, copy_len);
        *buf.add(copy_len) = 0; // NUL terminador
    }
    if !result.is_null() {
        unsafe { *result = copy_len };
    }
    napi_ok
}

/// Maior índice `<= max` que é fronteira de caractere UTF-8 em `bytes` (não
/// corta um code point no meio). `str::floor_char_boundary` é nightly, então
/// reimplementamos sobre os bytes.
fn floor_char_boundary(bytes: &[u8], max: usize) -> usize {
    if max >= bytes.len() {
        return bytes.len();
    }
    let mut i = max;
    // Byte de continuação UTF-8: 0b10xx_xxxx. Recua até um byte líder.
    while i > 0 && (bytes[i] & 0b1100_0000) == 0b1000_0000 {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn create_and_measure() {
        let s = b"caf\xc3\xa9"; // "café" UTF-8 (5 bytes)
        let mut v = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe {
                napi_create_string_utf8(env(), s.as_ptr() as *const c_char, s.len(), &mut v)
            },
            napi_ok
        );
        // Medição: buf=NULL → 5 bytes.
        let mut len = 0usize;
        assert_eq!(
            unsafe {
                napi_get_value_string_utf8(env(), v, ptr::null_mut(), 0, &mut len)
            },
            napi_ok
        );
        assert_eq!(len, 5);
    }

    #[test]
    fn copy_roundtrip() {
        let s = b"hello";
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_string_utf8(env(), s.as_ptr() as *const c_char, s.len(), &mut v) };
        let mut buf = [0i8; 16];
        let mut len = 0usize;
        unsafe { napi_get_value_string_utf8(env(), v, buf.as_mut_ptr(), 16, &mut len) };
        assert_eq!(len, 5);
        let got = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(got.to_str().unwrap(), "hello");
    }

    #[test]
    fn copy_truncates_at_char_boundary() {
        // "café" = 5 bytes; bufsize=5 → 4 bytes de payload (1 p/ NUL). O byte 4
        // (início do 'é' multibyte) seria cortado no meio → recua para 3.
        let s = b"caf\xc3\xa9";
        let mut v = napi_value(ptr::null_mut());
        unsafe { napi_create_string_utf8(env(), s.as_ptr() as *const c_char, s.len(), &mut v) };
        let mut buf = [0i8; 5];
        let mut len = 0usize;
        unsafe { napi_get_value_string_utf8(env(), v, buf.as_mut_ptr(), 5, &mut len) };
        // floor_char_boundary(café_bytes, 4) = 3 ("caf"), não corta o 'é'.
        assert_eq!(len, 3);
        let got = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(got.to_str().unwrap(), "caf");
    }

    #[test]
    fn auto_length() {
        let s = b"world\0extra"; // NUL no meio → strlen para em 5
        let mut v = napi_value(ptr::null_mut());
        unsafe {
            napi_create_string_utf8(env(), s.as_ptr() as *const c_char, NAPI_AUTO_LENGTH, &mut v)
        };
        let mut len = 0usize;
        unsafe { napi_get_value_string_utf8(env(), v, ptr::null_mut(), 0, &mut len) };
        assert_eq!(len, 5);
    }

    #[test]
    fn non_string_fails() {
        // typeof handle inválido → string_expected.
        let mut len = 0usize;
        let bogus = napi_value(0x1234 as *mut std::ffi::c_void);
        assert_eq!(
            unsafe { napi_get_value_string_utf8(env(), bogus, ptr::null_mut(), 0, &mut len) },
            napi_string_expected
        );
    }
}
