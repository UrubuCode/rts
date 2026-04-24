//! Plataforma / arquitetura / familia — constantes resolvidas em
//! compile-time via std::env::consts.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Nome canonico do SO: "windows", "linux", "macos", "ios", "android",
/// etc. Retorna string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_PLATFORM() -> u64 {
    intern(std::env::consts::OS)
}

/// Arquitetura: "x86_64", "aarch64", "x86", etc.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_ARCH() -> u64 {
    intern(std::env::consts::ARCH)
}

/// Familia: "unix" ou "windows".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_FAMILY() -> u64 {
    intern(std::env::consts::FAMILY)
}

/// Line ending convencional para o target: "\r\n" em Windows, "\n"
/// em Unix.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_EOL() -> u64 {
    #[cfg(target_os = "windows")]
    let eol: &str = "\r\n";
    #[cfg(not(target_os = "windows"))]
    let eol: &str = "\n";
    intern(eol)
}
