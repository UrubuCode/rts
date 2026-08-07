//! `os.cpus()` — one entry per logical core, with the per-core tick counters.
//!
//! # Why the counters are read from the OS rather than left at zero
//!
//! Because `times` is the only part of `CpuInfo` a program computes WITH:
//! model and speed are printed, but `times` is diffed across two calls to get
//! CPU utilisation, and a field that is always zero makes that answer `0%`
//! forever — a wrong answer that runs, which this crate refuses. Where the
//! counters cannot be read (a Unix that is not Linux) `cpus()` says so by
//! answering an EMPTY array, which Node itself documents as the failure answer,
//! rather than an array of plausible-looking zeros.

/// One logical core, as the OS reports it. Milliseconds, as Node's `CpuTimes`.
pub(super) struct Cpu {
    pub(super) model: String,
    pub(super) speed: f64,
    pub(super) user: f64,
    pub(super) nice: f64,
    pub(super) sys: f64,
    pub(super) idle: f64,
    pub(super) irq: f64,
}

/// Every logical core. Empty when the host will not report them.
#[cfg(windows)]
pub(super) fn cpus() -> Vec<Cpu> {
    // SystemProcessorPerformanceInformation. One record per logical core, and
    // the call reports how many it wrote — which is where the core COUNT comes
    // from, rather than a second query that could disagree with the array.
    const PROCESSOR_PERFORMANCE: u32 = 8;
    const MAX_CORES: usize = 512;
    let mut records = vec![ProcessorPerformance::default(); MAX_CORES];
    let bytes = (records.len() * std::mem::size_of::<ProcessorPerformance>()) as u32;
    let mut written = 0u32;
    let status = unsafe {
        NtQuerySystemInformation(PROCESSOR_PERFORMANCE, records.as_mut_ptr().cast(), bytes, &mut written)
    };
    if status != 0 {
        return Vec::new();
    }
    let count = written as usize / std::mem::size_of::<ProcessorPerformance>();
    records
        .into_iter()
        .take(count)
        .enumerate()
        .map(|(index, record)| {
            let subkey = format!(r"HARDWARE\DESCRIPTION\System\CentralProcessor\{index}");
            // The kernel counts in 100ns units; Node reports milliseconds.
            let ms = |ticks: i64| ticks as f64 / 10_000.0;
            Cpu {
                model: super::machine::registry_text(&subkey, "ProcessorNameString")
                    .unwrap_or_else(|| "unknown".to_owned()),
                speed: super::machine::registry_dword(&subkey, "~MHz").unwrap_or(0) as f64,
                user: ms(record.user),
                // Windows has no `nice`; Node reports 0 there rather than
                // omitting the field, and a program reading it wants a number.
                nice: 0.0,
                // KernelTime INCLUDES idle on Windows — libuv subtracts it, and
                // not subtracting it is how a machine at rest reports 100% sys.
                sys: ms(record.kernel - record.idle),
                idle: ms(record.idle),
                irq: ms(record.interrupt),
            }
        })
        .collect()
}

/// Every logical core, from `/proc/cpuinfo` (identity) and `/proc/stat`
/// (counters) — two files because Linux splits the two facts across them.
#[cfg(target_os = "linux")]
pub(super) fn cpus() -> Vec<Cpu> {
    let ticks_per_second = match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
        held if held > 0 => held as f64,
        // The USER_HZ every Linux ABI has used since the beginning; a sysconf
        // that refuses to answer is not a reason to report zero times.
        _ => 100.0,
    };
    let identities = cpuinfo();
    std::fs::read_to_string("/proc/stat")
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with("cpu") && !line.starts_with("cpu "))
        .enumerate()
        .map(|(index, line)| {
            let fields: Vec<f64> = line
                .split_whitespace()
                .skip(1)
                .map(|field| field.parse().unwrap_or(0.0))
                .collect();
            let at = |which: usize| fields.get(which).copied().unwrap_or(0.0) * 1000.0 / ticks_per_second;
            let (model, speed) = identities
                .get(index)
                .cloned()
                .unwrap_or_else(|| ("unknown".to_owned(), 0.0));
            Cpu {
                model,
                speed,
                user: at(0),
                nice: at(1),
                sys: at(2),
                idle: at(3),
                // `irq` and `softirq` are separate columns on Linux and one
                // field in Node's shape; libuv sums them, so this does too.
                irq: at(5) + at(6),
            }
        })
        .collect()
}

/// `(model, MHz)` per core, in `/proc/cpuinfo` order.
#[cfg(target_os = "linux")]
fn cpuinfo() -> Vec<(String, f64)> {
    let text = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let mut held = Vec::new();
    let mut model = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        match key.trim() {
            "model name" | "Model" => model = value.trim().to_owned(),
            "cpu MHz" => {
                held.push((model.clone(), value.trim().parse().unwrap_or(0.0)));
            }
            _ => {}
        }
    }
    // An ARM kernel reports no `cpu MHz` at all, so the loop above collected
    // nothing; the models are still real and the speed is genuinely unknown.
    if held.is_empty() && !model.is_empty() {
        held.push((model, 0.0));
    }
    held
}

/// No per-core source on this target — see the module doc for why this is an
/// empty array rather than an array of zeros.
#[cfg(all(not(windows), not(target_os = "linux")))]
pub(super) fn cpus() -> Vec<Cpu> {
    Vec::new()
}

/// The array `os.cpus()` answers, built in ONE borrow.
///
/// The reading happens before this is called and the building happens inside
/// it, which is the shape the borrow rule forces: `make_array_in` and
/// `make_object` take the context, and reaching for a registry key or a `/proc`
/// file while holding it would hold the runtime borrow across a syscall for no
/// reason.
pub(super) fn value(context: &mut rts_core_rwk::entry::Context, held: Vec<Cpu>) -> u64 {
    let entries = held
        .into_iter()
        .map(|cpu| {
            let object = rts_core_rwk::entry::make_object(context);
            let model = rts_core_rwk::entry::make_string(context, &cpu.model);
            rts_core_rwk::entry::put_member(context, object, "model", model);
            let speed = rts_core_rwk::entry::make_number(cpu.speed);
            rts_core_rwk::entry::put_member(context, object, "speed", speed);
            let times = rts_core_rwk::entry::make_object(context);
            for (name, value) in [
                ("user", cpu.user),
                ("nice", cpu.nice),
                ("sys", cpu.sys),
                ("idle", cpu.idle),
                ("irq", cpu.irq),
            ] {
                let number = rts_core_rwk::entry::make_number(value);
                rts_core_rwk::entry::put_member(context, times, name, number);
            }
            rts_core_rwk::entry::put_member(context, object, "times", times);
            object
        })
        .collect();
    rts_core_rwk::entry::make_array_in(context, entries)
}

/// The count `os.availableParallelism()` answers.
pub(super) fn parallelism() -> usize {
    std::thread::available_parallelism().map(|held| held.get()).unwrap_or(1)
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Default)]
#[allow(dead_code)]
struct ProcessorPerformance {
    idle: i64,
    kernel: i64,
    user: i64,
    /// Declared for the layout, not read: DPC time is already counted inside
    /// `kernel`, so adding it would double-count.
    dpc: i64,
    interrupt: i64,
    interrupt_count: u32,
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(class: u32, data: *mut u8, length: u32, written: *mut u32) -> i32;
}
