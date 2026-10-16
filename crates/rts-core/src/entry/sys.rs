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
