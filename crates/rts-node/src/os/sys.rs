//! node:os — shared low-level OS-identity syscalls used by more than one group
//! (`uname` powers `type`/`release`/`version`/`machine` and `words::node_type`;
//! hostname is its own function). Per-group syscalls (memory, cpus, priority,
//! network interfaces) live co-located in their group modules.
//!
//! POSIX: `libc::uname(2)` / `gethostname(2)`. Windows: `RtlGetVersion`
//! (ntdll — the accurate, non-`GetVersionEx`-shimmed build numbers) +
//! `GetComputerNameExW`, hand-declared `extern "system"` (repo idiom, no
//! `windows-sys` dependency — see `collector/scan.rs`). No fake/hardcoded
//! values: every field is the real running-kernel value.

/// Which `uname(3)` string field to read (POSIX only).
#[cfg(unix)]
#[derive(Clone, Copy)]
pub enum UnameField {
    /// `sysname` — OS name (`os.type()`), e.g. `"Linux"`, `"Darwin"`.
    Sysname,
    /// `release` — kernel release (`os.release()`), e.g. `"6.8.0-40-generic"`.
    Release,
    /// `version` — kernel version/build (`os.version()`).
    Version,
    /// `machine` — hardware identifier (`os.machine()`), e.g. `"x86_64"`.
    Machine,
}

#[cfg(unix)]
pub fn uname_field(field: UnameField) -> Option<String> {
    // SAFETY: `utsname` is POD; `uname` fully initializes it on success (0).
    let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::uname(&mut uts) };
    if rc != 0 {
        return None;
    }
    let arr: &[libc::c_char] = match field {
        UnameField::Sysname => &uts.sysname,
        UnameField::Release => &uts.release,
        UnameField::Version => &uts.version,
        UnameField::Machine => &uts.machine,
    };
    Some(c_char_arr_to_string(arr))
}

/// Decode a NUL-terminated `c_char` array (`utsname` field) to a Rust `String`.
#[cfg(unix)]
fn c_char_arr_to_string(arr: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = arr
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The OS hostname (`os.hostname()`). POSIX `gethostname(2)`; Windows
/// `GetComputerNameExW(ComputerNamePhysicalDnsHostname)`. Empty string if the
/// query fails (never fabricated).
pub fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = vec![0u8; 256];
        // SAFETY: buf is `len` bytes; gethostname writes at most `len`.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            return String::new();
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }
    #[cfg(windows)]
    {
        win::computer_name().unwrap_or_default()
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::new()
    }
}

/// `os.release()` — kernel release string.
pub fn release() -> String {
    #[cfg(unix)]
    {
        uname_field(UnameField::Release).unwrap_or_default()
    }
    #[cfg(windows)]
    {
        let v = win::os_version();
        // Node's os.release() on Windows is "major.minor.build".
        format!("{}.{}.{}", v.0, v.1, v.2)
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::new()
    }
}

/// `os.version()` — human-readable kernel/OS build identifier.
pub fn version() -> String {
    #[cfg(unix)]
    {
        uname_field(UnameField::Version).unwrap_or_default()
    }
    #[cfg(windows)]
    {
        let v = win::os_version();
        // Mirrors Node/libuv's Windows os.version(): "Windows <build>".
        format!("Windows {}.{}.{}", v.0, v.1, v.2)
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::new()
    }
}

/// `os.machine()` — raw `uname -m`-style hardware identifier (distinct enum
/// from `os.arch()`; e.g. `"x86_64"` vs `"x64"`).
pub fn machine() -> String {
    #[cfg(unix)]
    {
        uname_field(UnameField::Machine).unwrap_or_default()
    }
    #[cfg(windows)]
    {
        // Windows has no uname; map the compiled/native arch to the uname -m
        // enum Node reports (GetNativeSystemInfo would report the host arch;
        // for a natively-built binary the compiled arch matches it).
        match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "x86" => "i686",
            "aarch64" => "arm64",
            "arm" => "arm",
            other => other,
        }
        .to_string()
    }
    #[cfg(not(any(unix, windows)))]
    {
        String::new()
    }
}

#[cfg(windows)]
pub mod win {
    //! Windows identity syscalls: `RtlGetVersion` (accurate build numbers) and
    //! `GetComputerNameExW` (hostname). UTF-16 → `String` for the name.

    #[repr(C)]
    struct OsVersionInfoW {
        dw_os_version_info_size: u32,
        dw_major_version: u32,
        dw_minor_version: u32,
        dw_build_number: u32,
        dw_platform_id: u32,
        sz_csd_version: [u16; 128],
        w_service_pack_major: u16,
        w_service_pack_minor: u16,
        w_suite_mask: u16,
        w_product_type: u8,
        w_reserved: u8,
    }

    // `ComputerNamePhysicalDnsHostname` COMPUTER_NAME_FORMAT value.
    const COMPUTER_NAME_PHYSICAL_DNS_HOSTNAME: i32 = 5;

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }

    unsafe extern "system" {
        fn GetComputerNameExW(name_type: i32, buffer: *mut u16, size: *mut u32) -> i32;
    }

    /// `(major, minor, build)` of the running Windows, via `RtlGetVersion`
    /// (unlike `GetVersionEx`, not subject to app-compat manifest shimming).
    pub fn os_version() -> (u32, u32, u32) {
        let mut info: OsVersionInfoW = unsafe { std::mem::zeroed() };
        info.dw_os_version_info_size = std::mem::size_of::<OsVersionInfoW>() as u32;
        let rc = unsafe { RtlGetVersion(&mut info) };
        if rc != 0 {
            return (0, 0, 0);
        }
        (info.dw_major_version, info.dw_minor_version, info.dw_build_number)
    }

    /// The physical DNS hostname, or `None` on failure.
    pub fn computer_name() -> Option<String> {
        let mut size: u32 = 0;
        // First call: query required size (fails with buffer-too-small).
        unsafe {
            GetComputerNameExW(
                COMPUTER_NAME_PHYSICAL_DNS_HOSTNAME,
                std::ptr::null_mut(),
                &mut size,
            );
        }
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u16; size as usize];
        let rc = unsafe {
            GetComputerNameExW(
                COMPUTER_NAME_PHYSICAL_DNS_HOSTNAME,
                buf.as_mut_ptr(),
                &mut size,
            )
        };
        if rc == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}
