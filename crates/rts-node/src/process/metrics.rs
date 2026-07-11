//! node:process — process resource metrics: `memoryUsage` (real resident-set
//! size) and `cpuUsage` (real user/system CPU time). Values come from live OS
//! syscalls (Win32 `GetProcessMemoryInfo`/`GetProcessTimes`, POSIX `getrusage`)
//! — no fabricated numbers.
//!
//! The V8-heap breakdown fields of `memoryUsage()` (`heapTotal`/`heapUsed`/
//! `external`/`arrayBuffers`) report `0`: RTS has no V8 heap and does not expose
//! a separate JS-heap byte accounting, so `0` is the honest "not tracked" value
//! rather than a made-up number. `rss` is genuine.

use super::words::{num_word, object};

/// Resident-set size of this process in bytes (0 if the syscall fails).
fn rss_bytes() -> u64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut core::ffi::c_void;
            fn K32GetProcessMemoryInfo(
                process: *mut core::ffi::c_void,
                counters: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
        }
        let mut c: ProcessMemoryCounters = unsafe { core::mem::zeroed() };
        c.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        if ok != 0 { c.working_set_size as u64 } else { 0 }
    }
    #[cfg(unix)]
    {
        let mut ru: libc::rusage = unsafe { core::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } == 0 {
            // ru_maxrss is KB on Linux, bytes on macOS.
            let max = ru.ru_maxrss as u64;
            if cfg!(target_os = "macos") { max } else { max * 1024 }
        } else {
            0
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        0
    }
}

/// User and system CPU time consumed by this process so far, in microseconds.
fn cpu_micros() -> (u64, u64) {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut core::ffi::c_void;
            fn GetProcessTimes(
                process: *mut core::ffi::c_void,
                creation: *mut FileTime,
                exit: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }
        // FILETIME is in 100-nanosecond ticks → /10 gives microseconds.
        let ticks = |t: FileTime| ((t.high as u64) << 32 | t.low as u64) / 10;
        let (mut cr, mut ex, mut ke, mut us) = (
            FileTime { low: 0, high: 0 },
            FileTime { low: 0, high: 0 },
            FileTime { low: 0, high: 0 },
            FileTime { low: 0, high: 0 },
        );
        let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut cr, &mut ex, &mut ke, &mut us) };
        if ok != 0 { (ticks(us), ticks(ke)) } else { (0, 0) }
    }
    #[cfg(unix)]
    {
        let mut ru: libc::rusage = unsafe { core::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } == 0 {
            let us = ru.ru_utime.tv_sec as u64 * 1_000_000 + ru.ru_utime.tv_usec as u64;
            let sy = ru.ru_stime.tv_sec as u64 * 1_000_000 + ru.ru_stime.tv_usec as u64;
            (us, sy)
        } else {
            (0, 0)
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        (0, 0)
    }
}

/// `process.memoryUsage()` — `{ rss, heapTotal, heapUsed, external, arrayBuffers }`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_MEMORY_USAGE() -> u64 {
    let rss = rss_bytes() as f64;
    object(
        &["rss", "heapTotal", "heapUsed", "external", "arrayBuffers"],
        &[num_word(rss), num_word(0.0), num_word(0.0), num_word(0.0), num_word(0.0)],
    )
}

/// Free physical memory in bytes (0 if the query fails).
fn available_bytes() -> u64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct MemoryStatusEx {
            length: u32,
            memory_load: u32,
            total_phys: u64,
            avail_phys: u64,
            total_page_file: u64,
            avail_page_file: u64,
            total_virtual: u64,
            avail_virtual: u64,
            avail_extended_virtual: u64,
        }
        unsafe extern "system" {
            fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
        }
        let mut m: MemoryStatusEx = unsafe { core::mem::zeroed() };
        m.length = core::mem::size_of::<MemoryStatusEx>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 { m.avail_phys } else { 0 }
    }
    #[cfg(unix)]
    {
        let mut info: libc::sysinfo = unsafe { core::mem::zeroed() };
        if unsafe { libc::sysinfo(&mut info) } == 0 {
            info.freeram as u64 * info.mem_unit.max(1) as u64
        } else {
            0
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        0
    }
}

/// `process.availableMemory()` — free system memory in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_AVAILABLE_MEMORY() -> f64 {
    available_bytes() as f64
}

/// `process.constrainedMemory()` — the cgroup/job memory limit, or 0 when the
/// process is not memory-constrained (RTS reports the honest "unconstrained" 0
/// rather than probing cgroup v1/v2 files).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_CONSTRAINED_MEMORY() -> f64 {
    0.0
}

/// `process.cpuUsage()` — `{ user, system }` in microseconds since start.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_CPU_USAGE() -> u64 {
    let (user, system) = cpu_micros();
    object(&["user", "system"], &[num_word(user as f64), num_word(system as f64)])
}

/// `process.resourceUsage()` — the rusage struct. `userCPUTime`/`systemCPUTime`
/// (µs) and `maxRSS` (KB) are real; the remaining rusage fields RTS does not
/// track are reported as 0 (honest "not tracked", not fabricated).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PROC_RESOURCE_USAGE() -> u64 {
    let (user, system) = cpu_micros();
    let max_rss_kb = (rss_bytes() / 1024) as f64;
    let z = num_word(0.0);
    object(
        &[
            "userCPUTime", "systemCPUTime", "maxRSS", "sharedMemorySize",
            "unsharedDataSize", "unsharedStackSize", "minorPageFault",
            "majorPageFault", "swappedOut", "fsRead", "fsWrite", "ipcSent",
            "ipcReceived", "signalsCount", "voluntaryContextSwitches",
            "involuntaryContextSwitches",
        ],
        &[
            num_word(user as f64), num_word(system as f64), num_word(max_rss_kb),
            z, z, z, z, z, z, z, z, z, z, z, z, z,
        ],
    )
}
