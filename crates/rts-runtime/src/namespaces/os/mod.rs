//! `os` namespace — OS and environment info: platform, arch, special dirs.
//!
//! Implementacao sem deps externas. `home_dir` le HOME (Unix) / USERPROFILE
//! (Windows). `config_dir`/`cache_dir` seguem XDG no Unix com fallbacks; em
//! Windows usam APPDATA / LOCALAPPDATA.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`). All members return a GC string handle, so
//! each carries a `ts = "...: string"` override.

use rts_abi::ty::Handle;
use rts_macro::rts_namespace;

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

/// OS and environment info: platform, arch, special directories.
#[rts_namespace(os)]
impl OsNs {
    /// Canonical OS name: 'windows', 'linux', 'macos', 'ios', 'android', ...
    #[rts_fn(ts = "platform(): string")]
    pub fn platform() -> Handle {
        intern(std::env::consts::OS)
    }

    /// CPU architecture: 'x86_64', 'aarch64', 'x86', ...
    #[rts_fn(ts = "arch(): string")]
    pub fn arch() -> Handle {
        intern(std::env::consts::ARCH)
    }

    /// OS family: 'unix' or 'windows'.
    #[rts_fn(ts = "family(): string")]
    pub fn family() -> Handle {
        intern(std::env::consts::FAMILY)
    }

    /// Native line ending: '\r\n' on Windows, '\n' elsewhere.
    #[rts_fn(ts = "eol(): string")]
    pub fn eol() -> Handle {
        #[cfg(target_os = "windows")]
        let eol: &str = "\r\n";
        #[cfg(not(target_os = "windows"))]
        let eol: &str = "\n";
        intern(eol)
    }

    /// User home directory. Empty string if unresolvable.
    #[rts_fn(ts = "home_dir(): string")]
    pub fn home_dir() -> Handle {
        #[cfg(target_os = "windows")]
        let home = env_or_empty("USERPROFILE");
        #[cfg(not(target_os = "windows"))]
        let home = env_or_empty("HOME");
        intern(&home)
    }

    /// System temporary directory.
    #[rts_fn(ts = "temp_dir(): string")]
    pub fn temp_dir() -> Handle {
        let path = std::env::temp_dir();
        intern(&path.to_string_lossy())
    }

    /// Per-user config dir (%APPDATA% / XDG_CONFIG_HOME / ~/.config).
    #[rts_fn(ts = "config_dir(): string")]
    pub fn config_dir() -> Handle {
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
    #[rts_fn(ts = "cache_dir(): string")]
    pub fn cache_dir() -> Handle {
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
}
