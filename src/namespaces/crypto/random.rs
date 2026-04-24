//! CSPRNG — /dev/urandom em Unix, BCryptGenRandom em Windows.

use super::super::gc::handles::{table, Entry};

// ── Windows ──────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(
        hAlgorithm: *mut core::ffi::c_void,
        pbBuffer: *mut u8,
        cbBuffer: u32,
        dwFlags: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;

#[cfg(target_os = "windows")]
fn os_random_into(buf: &mut [u8]) -> bool {
    // SAFETY: chamada winapi padrao; ponteiro valido, flags corretas.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    status == 0
}

// ── Unix ─────────────────────────────────────────────────────────────
#[cfg(not(target_os = "windows"))]
fn os_random_into(buf: &mut [u8]) -> bool {
    use std::fs::File;
    use std::io::Read;
    match File::open("/dev/urandom") {
        Ok(mut f) => f.read_exact(buf).is_ok(),
        Err(_) => false,
    }
}

/// Preenche `len` bytes em `ptr` com dados CSPRNG. Retorna 0 em
/// sucesso, -1 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_RANDOM_BYTES(ptr: i64, len: i64) -> i64 {
    if ptr == 0 || len <= 0 {
        return -1;
    }
    // SAFETY: caller garante que ptr cobre `len` bytes validos
    // (tipicamente buffer.ptr(handle)).
    let slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) };
    if os_random_into(slice) { 0 } else { -1 }
}

/// Gera um u64 criptograficamente seguro. 0 pode aparecer como valor
/// valido; chame de novo se precisar "0 = erro" — nao ha canal de erro
/// separado aqui.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_RANDOM_I64() -> i64 {
    let mut bytes = [0u8; 8];
    if os_random_into(&mut bytes) {
        i64::from_le_bytes(bytes)
    } else {
        0
    }
}

/// Aloca um novo buffer (via namespaces::buffer) com `len` bytes
/// aleatorios. Retorna o handle, ou 0 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_CRYPTO_RANDOM_BUFFER(len: i64) -> u64 {
    if len < 0 {
        return 0;
    }
    let mut buf = vec![0u8; len as usize];
    if !os_random_into(&mut buf) {
        return 0;
    }
    table().lock().unwrap().alloc(Entry::Buffer(buf))
}
