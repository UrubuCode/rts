//! What the RUNNING kernel says about itself, plus the two live counters
//! (`memory`, `uptime`) — as plain Rust, with no engine value anywhere.
//!
//! # Why the OS calls live here rather than beside the natives
//!
//! Because the natives in `mod.rs` are the only place a `u64` engine value is
//! built, and the calls in here are the only place `unsafe` appears. Keeping
//! the two apart means the borrow rule (`with_runtime` may not nest) and the
//! FFI rule (a pointer is valid only while its allocation is) are each checked
//! in one file instead of interleaved in both.
//!
//! # Reuse check
//!
//! Nothing in `rts-cranelift` or `rts-core-rwk` answers an OS question — the
//! machine layer is about the compiled machine, not the host — and the only
//! sibling in this crate that reaches the host directly is `fs`, whose
//! `perms.rs`/`statfs.rs` hand-declare `extern "system"` Win32 entries over no
//! new dependency. That precedent is what this file follows rather than
//! inventing: `libc` for POSIX (already a dependency, FFI-declaration only per
//! `crates.md` §2) and hand-declared imports for Win32.

/// `uname -s`-equivalent, in Node's spelling.
pub(super) fn kind() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows_NT",
        "macos" => "Darwin",
        "linux" => "Linux",
        other => other,
    }
}

/// `os.release()` — the running kernel's version.
#[cfg(windows)]
pub(super) fn release() -> String {
    match version_info() {
        Some((major, minor, build)) => format!("{major}.{minor}.{build}"),
        None => String::new(),
    }
}

/// `os.version()` — the human-readable OS build identifier.
///
/// The registry `ProductName` is what Windows itself displays and what Node
/// reports here; a machine whose registry cannot be read answers the
/// `major.minor.build` triple instead of an invented product name.
///
/// # The Windows 11 correction is not cosmetic
///
/// `ProductName` still reads "Windows 10 Pro" on Windows 11 — Microsoft never
/// updated the key, and this machine proves it. libuv rewrites the leading
/// "Windows 10" once the build number reaches 22000, so Node answers "Windows
/// 11 Pro"; without the same rewrite a program branching on the version string
/// would decide it is on Windows 10. That is a wrong answer that runs, and the
/// build number is the OS's own fact rather than a guess.
#[cfg(windows)]
pub(super) fn version() -> String {
    let Some(product) = registry_text(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductName")
    else {
        return release();
    };
    match version_info() {
        Some((_, _, build)) if build >= 22_000 => product.replacen("Windows 10", "Windows 11", 1),
        _ => product,
    }
}

/// `uname -m`-equivalent — the machine name as the OS spells it.
///
/// Windows has no `uname`, and `GetNativeSystemInfo`'s numeric architecture is
/// the same fact the compiled target already carries for every configuration
/// RTS builds, so this maps the compiled target rather than adding a syscall
/// whose answer cannot differ.
pub(super) fn machine() -> String {
    #[cfg(windows)]
    {
        return match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "x86" => "i686",
            other => other,
        }
        .to_owned();
    }
    #[cfg(not(windows))]
    {
        uname_field(4).unwrap_or_default()
    }
}

/// `os.release()` — POSIX, `utsname.release`.
#[cfg(not(windows))]
pub(super) fn release() -> String {
    uname_field(2).unwrap_or_default()
}

/// `os.version()` — POSIX, `utsname.version`.
#[cfg(not(windows))]
pub(super) fn version() -> String {
    uname_field(3).unwrap_or_default()
}

/// One field of `struct utsname`, by its position in the struct: 0 `sysname`,
/// 1 `nodename`, 2 `release`, 3 `version`, 4 `machine`.
///
/// By index rather than five near-identical functions because the struct is
/// five fixed-size `char` arrays and the read is byte-for-byte the same for
/// each — five copies of it is five places for a NUL-termination bug.
#[cfg(not(windows))]
fn uname_field(which: usize) -> Option<String> {
    let mut info: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut info) } != 0 {
        return None;
    }
    let fields = [
        info.sysname.as_ptr(),
        info.nodename.as_ptr(),
        info.release.as_ptr(),
        info.version.as_ptr(),
        info.machine.as_ptr(),
    ];
    let start = *fields.get(which)?;
    // Safe because `uname` NUL-terminates every field it fills and the struct
    // was zeroed, so an untouched field is an empty string rather than a walk
    // off the end.
    Some(unsafe { std::ffi::CStr::from_ptr(start) }.to_string_lossy().into_owned())
}

/// `os.hostname()`.
///
/// `None` when the host will not say, which the caller reports as `""` rather
/// than substituting a name from the environment: `COMPUTERNAME`/`HOSTNAME` are
/// writable by the program's own parent and are therefore a claim about the
/// environment, not about the machine.
#[cfg(windows)]
pub(super) fn hostname() -> Option<String> {
    // ComputerNamePhysicalDnsHostname — the name libuv's `uv_os_gethostname`
    // asks for, so `os.hostname()` matches Node on a machine whose NetBIOS and
    // DNS names differ.
    const PHYSICAL_DNS_HOSTNAME: i32 = 5;
    let mut buffer = [0u16; 256];
    let mut size = buffer.len() as u32;
    let ok = unsafe { GetComputerNameExW(PHYSICAL_DNS_HOSTNAME, buffer.as_mut_ptr(), &mut size) };
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..size as usize]))
}

/// `os.hostname()` — POSIX `gethostname(2)`.
#[cfg(not(windows))]
pub(super) fn hostname() -> Option<String> {
    let mut buffer = vec![0u8; 256];
    let ok = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len() - 1) };
    if ok != 0 {
        return None;
    }
    let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(0);
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

/// `(total, free)` physical memory in bytes.
#[cfg(windows)]
pub(super) fn memory() -> Option<(f64, f64)> {
    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        ..MemoryStatusEx::default()
    };
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0)
        .then(|| (status.total_physical as f64, status.available_physical as f64))
}

/// `(total, free)` physical memory in bytes, from `/proc/meminfo`.
///
/// `MemAvailable` rather than `MemFree` for the free figure: that is what libuv
/// reports and what a program deciding whether it can allocate actually wants —
/// `MemFree` excludes reclaimable page cache and so understates it by gigabytes
/// on an ordinary machine.
#[cfg(target_os = "linux")]
pub(super) fn memory() -> Option<(f64, f64)> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let field = |key: &str| -> Option<f64> {
        let line = text.lines().find(|line| line.starts_with(key))?;
        let kib: f64 = line.trim_start_matches(key).trim().trim_end_matches(" kB").parse().ok()?;
        Some(kib * 1024.0)
    };
    Some((field("MemTotal:")?, field("MemAvailable:")?))
}

/// No `std`-only or `libc`-portable path to the figure on this target.
///
/// `None`, which the caller answers as `0`, rather than a number that looks
/// measured and is not — see the module doc's refusal list.
#[cfg(all(not(windows), not(target_os = "linux")))]
pub(super) fn memory() -> Option<(f64, f64)> {
    None
}

/// System uptime in seconds — since BOOT, which is what Node's name promises.
#[cfg(windows)]
pub(super) fn uptime() -> Option<f64> {
    Some(unsafe { GetTickCount64() } as f64 / 1000.0)
}

/// System uptime in seconds, from `/proc/uptime`'s first field.
#[cfg(target_os = "linux")]
pub(super) fn uptime() -> Option<f64> {
    let text = std::fs::read_to_string("/proc/uptime").ok()?;
    text.split_whitespace().next()?.parse().ok()
}

/// No boot-time source on this target.
#[cfg(all(not(windows), not(target_os = "linux")))]
pub(super) fn uptime() -> Option<f64> {
    None
}

/// The 1/5/15-minute load averages.
///
/// `[0, 0, 0]` on Windows unconditionally, and that is parity rather than a
/// gap: there is no Windows load-average concept, Node answers zeros, and
/// synthesizing one from CPU samples would be a different metric wearing this
/// one's name.
#[cfg(windows)]
pub(super) fn loadavg() -> [f64; 3] {
    [0.0, 0.0, 0.0]
}

/// The 1/5/15-minute load averages, from `getloadavg(3)`.
#[cfg(not(windows))]
pub(super) fn loadavg() -> [f64; 3] {
    let mut held = [0.0f64; 3];
    let count = unsafe { libc::getloadavg(held.as_mut_ptr(), 3) };
    match count {
        3 => held,
        _ => [0.0, 0.0, 0.0],
    }
}

/// The Windows major/minor/build triple, from `RtlGetVersion`.
///
/// `RtlGetVersion` rather than `GetVersionEx`, which lies: since Windows 8.1 it
/// reports 6.2 to any binary without a compatibility manifest, so `os.release()`
/// would answer "6.2.9200" on Windows 11.
#[cfg(windows)]
fn version_info() -> Option<(u32, u32, u32)> {
    let mut info = OsVersionInfoW {
        size: std::mem::size_of::<OsVersionInfoW>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    (unsafe { RtlGetVersion(&mut info) } == 0).then_some((info.major, info.minor, info.build))
}

/// One `REG_SZ` value under `HKEY_LOCAL_MACHINE`, as text.
#[cfg(windows)]
pub(super) fn registry_text(subkey: &str, value: &str) -> Option<String> {
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    let mut buffer = [0u8; 512];
    let mut size = buffer.len() as u32;
    let key = to_c_string(subkey);
    let name = to_c_string(value);
    let status = unsafe {
        RegGetValueA(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 || size == 0 {
        return None;
    }
    let end = (size as usize - 1).min(buffer.len() - 1);
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

/// One `REG_DWORD` value under `HKEY_LOCAL_MACHINE`.
#[cfg(windows)]
pub(super) fn registry_dword(subkey: &str, value: &str) -> Option<u32> {
    const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
    let mut held = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let key = to_c_string(subkey);
    let name = to_c_string(value);
    let status = unsafe {
        RegGetValueA(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut held).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(held)
}

/// A NUL-terminated byte vector, kept alive by the caller for the call.
#[cfg(windows)]
fn to_c_string(text: &str) -> Vec<i8> {
    text.bytes().map(|byte| byte as i8).chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
const HKEY_LOCAL_MACHINE: isize = -2_147_483_646; // 0x80000002 as a signed handle

/// The fields this reads are two of nine; the other seven are declared because
/// the LAYOUT is the contract — dropping an unread field would shift the two
/// that are read. That is why the `allow` is here rather than the fields being
/// deleted.
#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
#[allow(dead_code)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_physical: u64,
    available_physical: u64,
    total_page_file: u64,
    available_page_file: u64,
    total_virtual: u64,
    available_virtual: u64,
    available_extended_virtual: u64,
}

/// Same reason as [`MemoryStatusEx`]: `size` is written and never read back,
/// and the trailing service-pack text is what the read fields sit in front of.
#[cfg(windows)]
#[repr(C)]
#[allow(dead_code)]
struct OsVersionInfoW {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
    fn GetTickCount64() -> u64;
    fn GetComputerNameExW(kind: i32, buffer: *mut u16, size: *mut u32) -> i32;
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
}

#[cfg(windows)]
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueA(
        key: isize,
        subkey: *const i8,
        value: *const i8,
        flags: u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
}
