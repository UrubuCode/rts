//! macOS `os.cpus()` — model/speed via `sysctl`, per-core times via
//! `host_processor_info(PROCESSOR_CPU_LOAD_INFO)` (mach ticks → ms).

use super::{CpuInfo, CpuTimes};

// Stable mach CPU_STATE ABI indices (mach/processor_info.h).
const PROCESSOR_CPU_LOAD_INFO: libc::processor_flavor_t = 2;
const CPU_STATE_MAX: usize = 4;
const CPU_STATE_USER: usize = 0;
const CPU_STATE_SYSTEM: usize = 1;
const CPU_STATE_IDLE: usize = 2;
const CPU_STATE_NICE: usize = 3;

pub fn collect() -> Vec<CpuInfo> {
    let model = sysctl_string(b"machdep.cpu.brand_string\0").unwrap_or_default();
    let speed = sysctl_u64(b"hw.cpufrequency\0")
        .map(|hz| (hz / 1_000_000) as i64)
        .unwrap_or(0);
    let times = per_core_times();
    times
        .into_iter()
        .map(|t| CpuInfo {
            model: model.clone(),
            speed,
            times: t,
        })
        .collect()
}

fn per_core_times() -> Vec<CpuTimes> {
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    let multiplier = if hz > 0 { 1000.0 / hz as f64 } else { 10.0 };

    let mut ncpu: libc::natural_t = 0;
    let mut info: libc::processor_info_array_t = std::ptr::null_mut();
    let mut info_cnt: libc::mach_msg_type_number_t = 0;
    // SAFETY: mach host call; fills info/ncpu/info_cnt on success (KERN_SUCCESS).
    let rc = unsafe {
        libc::host_processor_info(
            libc::mach_host_self(),
            PROCESSOR_CPU_LOAD_INFO,
            &mut ncpu,
            &mut info,
            &mut info_cnt,
        )
    };
    if rc != 0 || info.is_null() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(ncpu as usize);
    for i in 0..ncpu as usize {
        let base = i * CPU_STATE_MAX;
        // SAFETY: info holds ncpu*CPU_STATE_MAX integers (info_cnt confirms).
        let tick = |state: usize| unsafe { *info.add(base + state) } as f64 * multiplier;
        out.push(CpuTimes {
            user: tick(CPU_STATE_USER),
            nice: tick(CPU_STATE_NICE),
            sys: tick(CPU_STATE_SYSTEM),
            idle: tick(CPU_STATE_IDLE),
            irq: 0.0,
        });
    }
    // SAFETY: release the vm region mach allocated for `info`.
    unsafe {
        libc::vm_deallocate(
            libc::mach_task_self(),
            info as libc::vm_address_t,
            (info_cnt as usize * std::mem::size_of::<libc::integer_t>()) as libc::vm_size_t,
        );
    }
    out
}

fn sysctl_u64(name: &[u8]) -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
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

fn sysctl_string(name: &[u8]) -> Option<String> {
    let mut len: usize = 0;
    // Size query.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const libc::c_char,
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const libc::c_char,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}
