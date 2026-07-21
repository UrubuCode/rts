//! `os` namespace — OS and environment info: platform, arch, special dirs.
//!
//! Implementacao sem deps externas. `home_dir` le HOME (Unix) / USERPROFILE
//! (Windows). `config_dir`/`cache_dir` seguem XDG no Unix com fallbacks; em
//! Windows usam APPDATA / LOCALAPPDATA.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime). Todos
//! os membros retornam um handle de string GC, então cada um carrega um
//! `ts = "...: string"` override.

use rts_engine::abi::ty::Handle;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns `s` into the GC string pool, returning its handle.
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// Canonical OS name: 'windows', 'linux', 'macos', 'ios', 'android', ...
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_PLATFORM() -> Handle {
    intern(std::env::consts::OS)
}

/// CPU architecture: 'x86_64', 'aarch64', 'x86', ...
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_ARCH() -> Handle {
    intern(std::env::consts::ARCH)
}

/// OS family: 'unix' or 'windows'.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_FAMILY() -> Handle {
    intern(std::env::consts::FAMILY)
}

/// Native line ending: '\r\n' on Windows, '\n' elsewhere.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_EOL() -> Handle {
    #[cfg(target_os = "windows")]
    let eol: &str = "\r\n";
    #[cfg(not(target_os = "windows"))]
    let eol: &str = "\n";
    intern(eol)
}

/// User home directory. Empty string if unresolvable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_HOME_DIR() -> Handle {
    #[cfg(target_os = "windows")]
    let home = env_or_empty("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let home = env_or_empty("HOME");
    intern(&home)
}

/// System temporary directory.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_TEMP_DIR() -> Handle {
    let path = std::env::temp_dir();
    intern(&path.to_string_lossy())
}

/// Per-user config dir (%APPDATA% / XDG_CONFIG_HOME / ~/.config).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_CONFIG_DIR() -> Handle {
    #[cfg(target_os = "windows")]
    let dir = env_or_empty("APPDATA");

    #[cfg(target_os = "macos")]
    let dir = {
        let home = env_or_empty("HOME");
        if home.is_empty() {
            String::new()
        } else {
            format!("{home}/Library/Application Support")
        }
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let dir = {
        let xdg = env_or_empty("XDG_CONFIG_HOME");
        if !xdg.is_empty() {
            xdg
        } else {
            let home = env_or_empty("HOME");
            if home.is_empty() {
                String::new()
            } else {
                format!("{home}/.config")
            }
        }
    };

    intern(&dir)
}

/// Per-user cache dir (%LOCALAPPDATA% / XDG_CACHE_HOME / ~/.cache).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_CACHE_DIR() -> Handle {
    #[cfg(target_os = "windows")]
    let dir = env_or_empty("LOCALAPPDATA");

    #[cfg(target_os = "macos")]
    let dir = {
        let home = env_or_empty("HOME");
        if home.is_empty() {
            String::new()
        } else {
            format!("{home}/Library/Caches")
        }
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let dir = {
        let xdg = env_or_empty("XDG_CACHE_HOME");
        if !xdg.is_empty() {
            xdg
        } else {
            let home = env_or_empty("HOME");
            if home.is_empty() {
                String::new()
            } else {
                format!("{home}/.cache")
            }
        }
    };

    intern(&dir)
}

/// Função `os.f()` retornando um handle de string GC.
fn func(name: &str, symbol: &str, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(Vec::new(), AbiType::Handle),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `os` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("os")
        .doc("OS and environment info: platform, arch, special directories.")
        .member(func(
            "platform",
            "__RTS_FN_NS_OS_PLATFORM",
            "platform(): string",
            "Canonical OS name: 'windows', 'linux', 'macos', 'ios', 'android', ...",
            __RTS_FN_NS_OS_PLATFORM as *const u8,
        ))
        .member(func(
            "arch",
            "__RTS_FN_NS_OS_ARCH",
            "arch(): string",
            "CPU architecture: 'x86_64', 'aarch64', 'x86', ...",
            __RTS_FN_NS_OS_ARCH as *const u8,
        ))
        .member(func(
            "family",
            "__RTS_FN_NS_OS_FAMILY",
            "family(): string",
            "OS family: 'unix' or 'windows'.",
            __RTS_FN_NS_OS_FAMILY as *const u8,
        ))
        .member(func(
            "eol",
            "__RTS_FN_NS_OS_EOL",
            "eol(): string",
            "Native line ending: '\\r\\n' on Windows, '\\n' elsewhere.",
            __RTS_FN_NS_OS_EOL as *const u8,
        ))
        .member(func(
            "home_dir",
            "__RTS_FN_NS_OS_HOME_DIR",
            "home_dir(): string",
            "User home directory. Empty string if unresolvable.",
            __RTS_FN_NS_OS_HOME_DIR as *const u8,
        ))
        .member(func(
            "temp_dir",
            "__RTS_FN_NS_OS_TEMP_DIR",
            "temp_dir(): string",
            "System temporary directory.",
            __RTS_FN_NS_OS_TEMP_DIR as *const u8,
        ))
        .member(func(
            "config_dir",
            "__RTS_FN_NS_OS_CONFIG_DIR",
            "config_dir(): string",
            "Per-user config dir (%APPDATA% / XDG_CONFIG_HOME / ~/.config).",
            __RTS_FN_NS_OS_CONFIG_DIR as *const u8,
        ))
        .member(func(
            "cache_dir",
            "__RTS_FN_NS_OS_CACHE_DIR",
            "cache_dir(): string",
            "Per-user cache dir (%LOCALAPPDATA% / XDG_CACHE_HOME / ~/.cache).",
            __RTS_FN_NS_OS_CACHE_DIR as *const u8,
        ))
        .done();
}
