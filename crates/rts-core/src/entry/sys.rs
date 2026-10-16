//! The native-host entry behind `import { totalmem, freemem } from "rts:sys"`.
//!
//! A host layer: it asks the operating system, so it exists only where there is
//! one. On `wasm32` the answer is `0`, and the compiler refuses `rts:sys` there
//! (`rts-codegen::runtime::core_names`) so no program reaches that arm.
//!
//! TODO: macOS answers `0` here (no `sysctl`/`host_statistics64` binding without
//! a libc dependency); `rts-node`'s `os.totalmem()` is the real one there.

/// Physical memory in bytes: `which` 0 is the total, 1 is what is available.
/// Any other selector answers `0`; the compiler only emits those two.
#[rtse::entry]
pub fn sys_memory(which: i64) -> f64 {
    match memory() {
        Some((total, free)) => match which {
            0 => total,
            1 => free,
            _ => 0.0,
        },
        None => 0.0,
    }
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
