//! The native-host entry behind `import { totalmem, freemem } from "rts:sys"`.
//!
//! A host layer: it asks the operating system, so it exists only where there is
//! one. On `wasm32` the answer is `0`, and the compiler refuses `rts:sys` there
//! (`rts-codegen::runtime::core_names`) so no program reaches that arm.
//!
//! TODO: macOS answers `0` here (no `sysctl`/`host_statistics64` binding without
//! a libc dependency); `rts-node`'s `os.totalmem()` is the real one there.

/// A numeric host query: `which` 0 is physical memory in bytes, 1 what is
/// available, 2 the uptime in seconds, 3 the logical CPU count. Any other
/// selector (and any failed read) answers `0`; the compiler only emits those four.
/// (Named for its first use; one entry with a selector keeps the table short.)
#[rtse::entry]
pub fn sys_memory(which: i64) -> f64 {
    match which {
        0 | 1 => match memory() {
            Some((total, free)) => if which == 0 { total } else { free },
            None => 0.0,
        },
        2 => uptime().unwrap_or(0.0),
        3 => cpu_count().unwrap_or(0.0),
        _ => 0.0,
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount64() -> u64;
    fn GetActiveProcessorCount(group: u16) -> u32;
}

/// Seconds since boot, from `GetTickCount64` (as libuv).
#[cfg(windows)]
fn uptime() -> Option<f64> {
    Some(unsafe { GetTickCount64() } as f64 / 1000.0)
}

/// Logical processors across every group (`ALL_PROCESSOR_GROUPS` = 0xFFFF).
#[cfg(windows)]
fn cpu_count() -> Option<f64> {
    let n = unsafe { GetActiveProcessorCount(0xFFFF) };
    (n > 0).then_some(n as f64)
}

/// Seconds since boot, the first field of `/proc/uptime`.
#[cfg(target_os = "linux")]
fn uptime() -> Option<f64> {
    std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse().ok()
}

/// Logical processors: the `cpuN` lines of `/proc/stat`.
#[cfg(target_os = "linux")]
fn cpu_count() -> Option<f64> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    let n = text
        .lines()
        .filter(|l| l.strip_prefix("cpu").is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit())))
        .count();
    (n > 0).then_some(n as f64)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn uptime() -> Option<f64> {
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
fn cpu_count() -> Option<f64> {
    std::thread::available_parallelism().ok().map(|n| n.get() as f64)
}

#[cfg(windows)]
#[repr(C)]
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

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
}

/// `(total, available)` bytes, from `GlobalMemoryStatusEx`.
#[cfg(windows)]
fn memory() -> Option<(f64, f64)> {
    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_physical: 0,
        available_physical: 0,
        total_page_file: 0,
        available_page_file: 0,
        total_virtual: 0,
        available_virtual: 0,
        available_extended_virtual: 0,
    };
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0)
        .then(|| (status.total_physical as f64, status.available_physical as f64))
}

/// `(total, available)` bytes, from `/proc/meminfo` (`MemAvailable`, as libuv).
#[cfg(target_os = "linux")]
fn memory() -> Option<(f64, f64)> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let field = |key: &str| -> Option<f64> {
        let line = text.lines().find(|line| line.starts_with(key))?;
        let kib: f64 = line.trim_start_matches(key).trim().trim_end_matches(" kB").parse().ok()?;
        Some(kib * 1024.0)
    };
    Some((field("MemTotal:")?, field("MemAvailable:")?))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn memory() -> Option<(f64, f64)> {
    None
}

/// A textual host query: `which` 0 is `os.hostname()`, 1 `os.homedir()`, 2
/// `os.tmpdir()`. Returns an interned string; any other selector (and any
/// failed read) answers the empty string, and the compiler only emits those
/// three. Unlike [`sys_memory`] it allocates, so it is NOT in the codegen's
/// `CANNOT_RAISE` set.
#[rtse::entry]
pub fn sys_text(which: i64) -> u64 {
    let text = match which {
        0 => hostname(),
        1 => homedir(),
        2 => Some(tmpdir()),
        _ => None,
    }
    .unwrap_or_default();
    // The string is built before the context is borrowed: reading the OS holds
    // no heap state.
    super::with_current(|context| context.intern_value(crate::text::Str::from_str(&text)).bits())
}

fn env_text(name: &str) -> Option<String> {
    let value = std::env::var_os(name)?.to_string_lossy().into_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetComputerNameExW(kind: u32, buffer: *mut u16, size: *mut u32) -> i32;
}

/// The DNS host name (`ComputerNameDnsHostname`), which is what libuv's
/// `uv_os_gethostname` answers.
#[cfg(windows)]
fn hostname() -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    (unsafe { GetComputerNameExW(1, buffer.as_mut_ptr(), &mut size) } != 0)
        .then(|| String::from_utf16_lossy(&buffer[..size as usize]))
}

#[cfg(not(windows))]
fn hostname() -> Option<String> {
    let text = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text).or_else(|| env_text("HOSTNAME"))
}

/// `USERPROFILE` on Windows, `HOME` elsewhere: libuv's first choice, which is
/// what `os.homedir()` answers in practice.
fn homedir() -> Option<String> {
    if cfg!(windows) { env_text("USERPROFILE") } else { env_text("HOME") }
}

/// Node's `os.tmpdir()`: the first set of `TEMP`/`TMP` (Windows) or
/// `TMPDIR`/`TMP`/`TEMP` (elsewhere), else `SystemRoot\temp` / `/tmp`, with one
/// trailing separator removed (but not from `C:\` or `/`).
fn tmpdir() -> String {
    let mut path = if cfg!(windows) {
        env_text("TEMP")
            .or_else(|| env_text("TMP"))
            .or_else(|| env_text("SystemRoot").or_else(|| env_text("windir")).map(|root| format!("{root}\\temp")))
            .unwrap_or_default()
    } else {
        env_text("TMPDIR").or_else(|| env_text("TMP")).or_else(|| env_text("TEMP")).unwrap_or_else(|| "/tmp".to_string())
    };
    if cfg!(windows) {
        if path.len() > 1 && path.ends_with('\\') && !path.ends_with(":\\") {
            path.pop();
        }
    } else if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path
}
