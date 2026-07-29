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

use rts_engine::Engine;

fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// Canonical OS name: 'windows', 'linux', 'macos', 'ios', 'android', ...
#[rtse::function(module = "os", value = "platform")]
fn platform() -> String {
    std::env::consts::OS.to_string()
}

/// CPU architecture: 'x86_64', 'aarch64', 'x86', ...
#[rtse::function(module = "os", value = "arch")]
fn arch() -> String {
    std::env::consts::ARCH.to_string()
}

/// OS family: 'unix' or 'windows'.
#[rtse::function(module = "os", value = "family")]
fn family() -> String {
    std::env::consts::FAMILY.to_string()
}

/// Native line ending: '\r\n' on Windows, '\n' elsewhere.
#[rtse::function(module = "os", value = "eol")]
fn eol() -> String {
    #[cfg(target_os = "windows")]
    let eol: &str = "\r\n";
    #[cfg(not(target_os = "windows"))]
    let eol: &str = "\n";
    eol.to_string()
}

/// User home directory. Empty string if unresolvable.
#[rtse::function(module = "os", value = "home_dir")]
fn home_dir() -> String {
    #[cfg(target_os = "windows")]
    let home = env_or_empty("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let home = env_or_empty("HOME");
    home
}

/// System temporary directory.
#[rtse::function(module = "os", value = "temp_dir")]
fn temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().to_string()
}

/// Per-user config dir (%APPDATA% / XDG_CONFIG_HOME / ~/.config).
#[rtse::function(module = "os", value = "config_dir")]
fn config_dir() -> String {
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

    dir
}

/// Per-user cache dir (%LOCALAPPDATA% / XDG_CACHE_HOME / ~/.cache).
#[rtse::function(module = "os", value = "cache_dir")]
fn cache_dir() -> String {
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

    dir
}

/// Registra a namespace `os` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.module("os", |m| {
        m.doc("OS and environment info: platform, arch, special directories.");
        m.registry(platform_entry());
        m.registry(arch_entry());
        m.registry(family_entry());
        m.registry(eol_entry());
        m.registry(home_dir_entry());
        m.registry(temp_dir_entry());
        m.registry(config_dir_entry());
        m.registry(cache_dir_entry());
    });
}
