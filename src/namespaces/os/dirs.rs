//! Diretorios especiais do usuario — home, temp, config, cache.
//!
//! Implementacao sem deps externas. `home_dir` le HOME (Unix) ou
//! USERPROFILE (Windows). `config_dir`/`cache_dir` seguem XDG no
//! Unix com fallbacks; em Windows usam APPDATA / LOCALAPPDATA.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// Home do usuario. Vazio se nao conseguir resolver.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_HOME_DIR() -> u64 {
    #[cfg(target_os = "windows")]
    let home = env_or_empty("USERPROFILE");
    #[cfg(not(target_os = "windows"))]
    let home = env_or_empty("HOME");
    intern(&home)
}

/// Diretorio temporario do sistema.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_TEMP_DIR() -> u64 {
    let path = std::env::temp_dir();
    intern(&path.to_string_lossy())
}

/// Config dir do usuario.
/// - Windows: %APPDATA% (ex: C:\Users\foo\AppData\Roaming)
/// - macOS:   $HOME/Library/Application Support
/// - Linux:   $XDG_CONFIG_HOME ou $HOME/.config
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_CONFIG_DIR() -> u64 {
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

/// Cache dir do usuario.
/// - Windows: %LOCALAPPDATA%
/// - macOS:   $HOME/Library/Caches
/// - Linux:   $XDG_CACHE_HOME ou $HOME/.cache
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_OS_CACHE_DIR() -> u64 {
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
