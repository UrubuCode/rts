//! Windows `os.cpus()` — per-core times via
//! `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` (100ns
//! units → ms) and model/`~MHz` from the `CentralProcessor` registry keys.

use super::{CpuInfo, CpuTimes};

#[repr(C)]
#[derive(Clone, Copy)]
struct SystemInfo {
    // Only the fields we read; the union/anon leading members are covered by
    // the reserved words so the layout/size matches the Win32 SYSTEM_INFO.
    _oem_and_arch: u32,
    _dw_page_size: u32,
    _lp_min_app_addr: usize,
    _lp_max_app_addr: usize,
    _dw_active_mask: usize,
    dw_number_of_processors: u32,
    _dw_processor_type: u32,
    _dw_alloc_granularity: u32,
    _w_proc_level: u16,
    _w_proc_rev: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcPerfInfo {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: u32 = 8;

unsafe extern "system" {
    fn GetSystemInfo(info: *mut SystemInfo);
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(
        class: u32,
        info: *mut core::ffi::c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> i32;
}

pub fn collect() -> Vec<CpuInfo> {
    let count = processor_count();
    if count == 0 {
        return Vec::new();
    }
    let perf = query_perf(count);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let (model, speed) = reg::model_and_speed(i);
        let times = perf.get(i).map(to_times).unwrap_or_default();
        out.push(CpuInfo { model, speed, times });
    }
    out
}

fn processor_count() -> usize {
    let mut info: SystemInfo = unsafe { std::mem::zeroed() };
    unsafe { GetSystemInfo(&mut info) };
    info.dw_number_of_processors as usize
}

fn query_perf(count: usize) -> Vec<ProcPerfInfo> {
    let mut buf = vec![ProcPerfInfo::default(); count];
    let bytes = (count * std::mem::size_of::<ProcPerfInfo>()) as u32;
    let mut ret_len: u32 = 0;
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            bytes,
            &mut ret_len,
        )
    };
    if status != 0 {
        return Vec::new();
    }
    buf
}

/// 100ns ticks → ms; `sys = kernel − idle` (KernelTime includes idle).
fn to_times(p: &ProcPerfInfo) -> CpuTimes {
    const HUNDRED_NS_TO_MS: f64 = 1.0 / 10_000.0;
    CpuTimes {
        user: p.user_time as f64 * HUNDRED_NS_TO_MS,
        nice: 0.0,
        sys: (p.kernel_time - p.idle_time) as f64 * HUNDRED_NS_TO_MS,
        idle: p.idle_time as f64 * HUNDRED_NS_TO_MS,
        irq: p.interrupt_time as f64 * HUNDRED_NS_TO_MS,
    }
}

mod reg {
    //! Read `ProcessorNameString` (REG_SZ) and `~MHz` (REG_DWORD) from
    //! `HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\<n>`.

    type Hkey = isize;
    const HKEY_LOCAL_MACHINE: Hkey = 0x8000_0002u32 as i32 as isize;
    const KEY_READ: u32 = 0x2_0019;
    const ERROR_SUCCESS: i32 = 0;
    const REG_SZ: u32 = 1;
    const REG_DWORD: u32 = 4;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            key: Hkey,
            subkey: *const u16,
            options: u32,
            desired: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            value: *const u16,
            reserved: *mut u32,
            ty: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }

    fn to_utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// `(model, speed_mhz)` for core `index`, defaulting to `("", 0)`.
    pub fn model_and_speed(index: usize) -> (String, i64) {
        let path = to_utf16(&format!(
            "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\{index}"
        ));
        let mut key: Hkey = 0;
        let rc = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                0,
                KEY_READ,
                &mut key,
            )
        };
        if rc != ERROR_SUCCESS {
            return (String::new(), 0);
        }
        let model = read_string(key, "ProcessorNameString").unwrap_or_default();
        let speed = read_dword(key, "~MHz").unwrap_or(0) as i64;
        unsafe { RegCloseKey(key) };
        (model, speed)
    }

    fn read_string(key: Hkey, value: &str) -> Option<String> {
        let name = to_utf16(value);
        let mut ty: u32 = 0;
        let mut len: u32 = 0;
        // Size query.
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if rc != ERROR_SUCCESS || ty != REG_SZ || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        // Reinterpret the byte buffer as UTF-16, dropping the trailing NUL.
        let u16s: Vec<u16> = buf[..len as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let end = u16s.iter().position(|&c| c == 0).unwrap_or(u16s.len());
        Some(String::from_utf16_lossy(&u16s[..end]).trim().to_string())
    }

    fn read_dword(key: Hkey, value: &str) -> Option<u32> {
        let name = to_utf16(value);
        let mut ty: u32 = 0;
        let mut data: u32 = 0;
        let mut len: u32 = std::mem::size_of::<u32>() as u32;
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                &mut data as *mut u32 as *mut u8,
                &mut len,
            )
        };
        if rc == ERROR_SUCCESS && ty == REG_DWORD {
            Some(data)
        } else {
            None
        }
    }
}
