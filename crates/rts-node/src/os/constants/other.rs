//! `os.constants.{dlopen, priority, libuv}` — `dlopen` from `libc` on POSIX
//! (empty on Windows, no `dlopen(3)`); `priority`/`libuv` are Node-level
//! conventions, platform-independent.

/// `os.constants.dlopen` — `RTLD_*` flags (POSIX only; empty on Windows).
pub fn dlopen_entries() -> Vec<(&'static str, i64)> {
    #[cfg(unix)]
    {
        let mut v: Vec<(&'static str, i64)> = vec![
            ("RTLD_LAZY", libc::RTLD_LAZY as i64),
            ("RTLD_NOW", libc::RTLD_NOW as i64),
            ("RTLD_GLOBAL", libc::RTLD_GLOBAL as i64),
            ("RTLD_LOCAL", libc::RTLD_LOCAL as i64),
        ];
        #[cfg(target_os = "linux")]
        {
            v.push(("RTLD_DEEPBIND", libc::RTLD_DEEPBIND as i64));
        }
        v
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

/// `os.constants.priority` — the 6 fixed `PRIORITY_*` nice values.
pub fn priority_entries() -> Vec<(&'static str, i64)> {
    vec![
        ("PRIORITY_LOW", 19),
        ("PRIORITY_BELOW_NORMAL", 10),
        ("PRIORITY_NORMAL", 0),
        ("PRIORITY_ABOVE_NORMAL", -7),
        ("PRIORITY_HIGH", -14),
        ("PRIORITY_HIGHEST", -20),
    ]
}

/// `os.constants.libuv` — the single exposed libuv flag mirror.
pub fn libuv_entries() -> Vec<(&'static str, i64)> {
    vec![("UV_UDP_REUSEADDR", 4)]
}
