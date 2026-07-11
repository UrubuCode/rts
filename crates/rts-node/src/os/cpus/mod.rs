//! node:os — `os.cpus()`: one `{ model, speed, times }` object per logical core.
//!
//! Real per-core data: Linux `/proc/cpuinfo` + `/proc/stat`; Windows
//! `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` + registry
//! model/`~MHz`; macOS `sysctl` + `host_processor_info`. `times` are cumulative
//! milliseconds (`user`/`nice`/`sys`/`idle`/`irq`). Returns `[]` (never a
//! fabricated core) when the OS exposes no per-core detail.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use super::words::{array, num_word, object_word, str_word};

/// One logical core's info.
pub struct CpuInfo {
    pub model: String,
    /// Clock speed in MHz (`0` when undeterminable).
    pub speed: i64,
    pub times: CpuTimes,
}

/// Cumulative CPU time (milliseconds) in each mode.
#[derive(Default, Clone, Copy)]
pub struct CpuTimes {
    pub user: f64,
    pub nice: f64,
    pub sys: f64,
    pub idle: f64,
    pub irq: f64,
}

/// `os.cpus()` — array of per-core info objects.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_OS_CPUS() -> u64 {
    let cores = collect();
    let words: Vec<i64> = cores.iter().map(cpu_word).collect();
    array(words)
}

/// Encode one `CpuInfo` as an OBJECT element word.
fn cpu_word(c: &CpuInfo) -> i64 {
    let t = &c.times;
    let times = object_word(
        &["user", "nice", "sys", "idle", "irq"],
        &[
            num_word(t.user),
            num_word(t.nice),
            num_word(t.sys),
            num_word(t.idle),
            num_word(t.irq),
        ],
    );
    object_word(
        &["model", "speed", "times"],
        &[str_word(&c.model), num_word(c.speed as f64), times],
    )
}

fn collect() -> Vec<CpuInfo> {
    #[cfg(target_os = "linux")]
    {
        linux::collect()
    }
    #[cfg(target_os = "macos")]
    {
        macos::collect()
    }
    #[cfg(windows)]
    {
        windows::collect()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        Vec::new()
    }
}
