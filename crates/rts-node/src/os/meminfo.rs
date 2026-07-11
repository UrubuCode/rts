//! node:os — memory, uptime, load average, available parallelism.
//!
//! Real syscalls only: Linux `sysinfo(2)`, macOS `sysctl`/`host_statistics64`,
//! Windows `GlobalMemoryStatusEx`/`GetTickCount64`; load average via POSIX
//! `getloadavg(3)` (`[0,0,0]` on Windows, matching Node — no synthesized
//! metric). `availableParallelism()` is affinity-aware
//! (`std::thread::available_parallelism`) and additionally cgroup-quota-aware
//! on Linux, matching libuv's container-aware behavior. No fabricated numbers.

use super::words::{array, num_word};

/// `os.totalmem()` — total physical memory in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_TOTALMEM() -> f64 {
    total_memory_bytes() as f64
}

/// `os.freemem()` — free physical memory in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_FREEMEM() -> f64 {
    free_memory_bytes() as f64
}

/// `os.uptime()` — system uptime in seconds (may be fractional).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_UPTIME() -> f64 {
    uptime_seconds()
}

/// `os.loadavg()` — `[1min, 5min, 15min]` load averages (all `0` on Windows).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_LOADAVG() -> u64 {
    let [a, b, c] = load_average();
    array(vec![num_word(a), num_word(b), num_word(c)])
}

/// `os.availableParallelism()` — parallelism estimate honoring CPU affinity
/// and (on Linux) cgroup CPU quota. Always `>= 1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_AVAILABLE_PARALLELISM() -> f64 {
    available_parallelism() as f64
}

// --- platform implementations ------------------------------------------------

fn total_memory_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        linux_sysinfo().map(|si| si.totalram as u64 * si.mem_unit.max(1) as u64).unwrap_or(0)
    }
    #[cfg(target_os = "macos")]
    {
        macos::sysctl_u64(b"hw.memsize\0").unwrap_or(0)
    }
    #[cfg(windows)]
    {
        win::memory_status().map(|m| m.ull_total_phys).unwrap_or(0)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        0
    }
}

pub(crate) fn free_memory_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        linux_sysinfo().map(|si| si.freeram as u64 * si.mem_unit.max(1) as u64).unwrap_or(0)
    }
    #[cfg(target_os = "macos")]
    {
        macos::free_memory().unwrap_or(0)
    }
    #[cfg(windows)]
    {
        win::memory_status().map(|m| m.ull_avail_phys).unwrap_or(0)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        0
    }
}

fn uptime_seconds() -> f64 {
    #[cfg(target_os = "linux")]
    {
        linux_sysinfo().map(|si| si.uptime as f64).unwrap_or(0.0)
    }
    #[cfg(target_os = "macos")]
    {
        macos::uptime().unwrap_or(0.0)
    }
    #[cfg(windows)]
    {
        (win::tick_count64() as f64) / 1000.0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        0.0
    }
}

fn load_average() -> [f64; 3] {
    #[cfg(unix)]
    {
        let mut loads = [0.0f64; 3];
        // SAFETY: writes exactly 3 f64 into the caller-provided buffer.
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        if n == 3 {
            loads
        } else {
            [0.0, 0.0, 0.0]
        }
    }
    #[cfg(not(unix))]
    {
        [0.0, 0.0, 0.0]
    }
}

fn available_parallelism() -> usize {
    let base = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    #[cfg(target_os = "linux")]
    {
        match linux_cgroup_cpu_quota() {
            Some(q) if q >= 1 => base.min(q),
            _ => base,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        base
    }
}

// --- Linux -------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn linux_sysinfo() -> Option<libc::sysinfo> {
    // SAFETY: sysinfo fully initializes the struct on success (0).
    let mut si: libc::sysinfo = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::sysinfo(&mut si) };
    if rc == 0 {
        Some(si)
    } else {
        None
    }
}

/// cgroup CPU quota → whole-CPU count (v2 `cpu.max`, then v1
/// `cpu.cfs_quota_us`/`cpu.cfs_period_us`). `None` when unlimited/unset.
#[cfg(target_os = "linux")]
fn linux_cgroup_cpu_quota() -> Option<usize> {
    // cgroup v2: "<quota> <period>" ("max <period>" = unlimited).
    if let Ok(s) = std::fs::read_to_string("/sys/fs/cgroup/cpu.max") {
        let mut it = s.split_whitespace();
        if let (Some(q), Some(p)) = (it.next(), it.next()) {
            if q != "max" {
                if let (Ok(quota), Ok(period)) = (q.parse::<i64>(), p.parse::<i64>()) {
                    if quota > 0 && period > 0 {
                        return Some(((quota + period - 1) / period).max(1) as usize);
                    }
                }
            }
            return None;
        }
    }
    // cgroup v1.
    let quota = read_i64("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")?;
    let period = read_i64("/sys/fs/cgroup/cpu/cpu.cfs_period_us")?;
    if quota > 0 && period > 0 {
        Some(((quota + period - 1) / period).max(1) as usize)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn read_i64(path: &str) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.trim().parse::<i64>().ok()
}

// --- macOS -------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    /// `sysctlbyname(name)` reading a single `u64` value (e.g. `hw.memsize`).
    pub fn sysctl_u64(name: &[u8]) -> Option<u64> {
        let mut value: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        // SAFETY: name is NUL-terminated; value/len match the queried scalar.
        let rc = unsafe {
            libc::sysctlbyname(
                name.as_ptr() as *const libc::c_char,
                &mut value as *mut u64 as *mut libc::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc == 0 {
            Some(value)
        } else {
            None
        }
    }

    /// Free memory: `(free + inactive) * pagesize` from `host_statistics64`
    /// (a real VM figure, matching what `top` reports as available).
    pub fn free_memory() -> Option<u64> {
        let pagesize = sysctl_u64(b"hw.pagesize\0").unwrap_or(4096);
        let mut stats: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
        let mut count = (std::mem::size_of::<libc::vm_statistics64>()
            / std::mem::size_of::<libc::integer_t>()) as libc::mach_msg_type_number_t;
        // SAFETY: HOST_VM_INFO64 fills vm_statistics64; count sized accordingly.
        let rc = unsafe {
            libc::host_statistics64(
                libc::mach_host_self(),
                libc::HOST_VM_INFO64,
                &mut stats as *mut _ as *mut libc::integer_t,
                &mut count,
            )
        };
        if rc != 0 {
            return None;
        }
        let free_pages = stats.free_count as u64 + stats.inactive_count as u64;
        Some(free_pages * pagesize)
    }

    /// Uptime = now − `kern.boottime`.
    pub fn uptime() -> Option<f64> {
        let mut tv: libc::timeval = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of::<libc::timeval>();
        let mut mib = [libc::CTL_KERN, libc::KERN_BOOTTIME];
        let rc = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                mib.len() as libc::c_uint,
                &mut tv as *mut _ as *mut libc::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 {
            return None;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs_f64();
        Some((now - tv.tv_sec as f64).max(0.0))
    }
}

// --- Windows -----------------------------------------------------------------

#[cfg(windows)]
mod win {
    #[repr(C)]
    pub struct MemoryStatusEx {
        pub dw_length: u32,
        pub dw_memory_load: u32,
        pub ull_total_phys: u64,
        pub ull_avail_phys: u64,
        pub ull_total_page_file: u64,
        pub ull_avail_page_file: u64,
        pub ull_total_virtual: u64,
        pub ull_avail_virtual: u64,
        pub ull_avail_extended_virtual: u64,
    }

    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
        fn GetTickCount64() -> u64;
    }

    pub fn memory_status() -> Option<MemoryStatusEx> {
        let mut m: MemoryStatusEx = unsafe { std::mem::zeroed() };
        m.dw_length = std::mem::size_of::<MemoryStatusEx>() as u32;
        let rc = unsafe { GlobalMemoryStatusEx(&mut m) };
        if rc != 0 {
            Some(m)
        } else {
            None
        }
    }

    pub fn tick_count64() -> u64 {
        unsafe { GetTickCount64() }
    }
}
