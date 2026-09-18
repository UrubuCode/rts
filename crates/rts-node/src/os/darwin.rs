//! The macOS answers to `os.cpus()`, `os.totalmem()`, `os.freemem()`,
//! `os.uptime()` and — through `machine::memory` — `process.availableMemory()`.
//!
//! # Why a module of its own
//!
//! macOS answers none of these from a file the way Linux does from `/proc`:
//! each is a `sysctl` or a Mach host call, and the helpers that read a `sysctl`
//! are shared by all four. Written into `machine.rs` and `cpus.rs` they would
//! be two copies of the same `sysctlbyname` dance behind two `cfg`s.
//!
//! # What each answer is, and why it is that one
//!
//! Every figure is the one libuv computes on darwin, because what a program
//! compares it against is Node:
//!
//! - **total memory** is `hw.memsize`;
//! - **free memory** is `free_count × page size` from `host_statistics64` —
//!   libuv's `uv_get_free_memory`, and `uv_get_available_memory` is that same
//!   function on darwin, so `process.availableMemory()` and `os.freemem()`
//!   answer one number, as they do in Node. The page size is `sysconf`'s, which
//!   is 16 KiB on Apple Silicon: a hard-coded 4 KiB would under-report by four;
//! - **uptime** is `now − kern.boottime`, in whole seconds, as libuv's;
//! - **cpus** is one entry per `host_processor_info` record, which is one per
//!   LOGICAL core — the same count `hw.ncpu` and `os.availableParallelism()`
//!   answer. Before this module the array was empty, which is Node's documented
//!   failure answer, and `os.cpus()[0].model` threw.
//!
//! # What is NOT exact
//!
//! `speed` is `hw.cpufrequency` in MHz where the kernel publishes it (Intel
//! Macs) and `0` where it does not (Apple Silicon). Recent libuv asks IOKit for
//! the performance cluster's top frequency there; that needs a framework this
//! crate does not link, and `0` is the same "genuinely unknown" answer the ARM
//! Linux branch of `cpus.rs` already gives, rather than a plausible figure.
//! `irq` is `0` because Mach does not count interrupt time per core — libuv
//! answers `0` too.

use std::ffi::CStr;
use std::ptr::null_mut;

use super::cpus::Cpu;

/// The raw bytes of a `sysctl` value, sized by asking first.
fn sysctl_bytes(name: &CStr) -> Option<Vec<u8>> {
    let mut size = 0usize;
    // SAFETY: a size query — `oldp` null, `oldlenp` a valid local — which is
    // the documented way to learn a value's length before reading it.
    let asked = unsafe { libc::sysctlbyname(name.as_ptr(), null_mut(), &mut size, null_mut(), 0) };
    if asked != 0 || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size];
    // SAFETY: `buffer` is `size` bytes long and `size` says so; the kernel
    // writes at most that many and updates `size` to what it wrote.
    let read = unsafe {
        libc::sysctlbyname(name.as_ptr(), buffer.as_mut_ptr().cast(), &mut size, null_mut(), 0)
    };
    if read != 0 {
        return None;
    }
    buffer.truncate(size);
    Some(buffer)
}

/// An integer `sysctl`. The kernel declares some as 32-bit and some as 64-bit
/// (`hw.memsize` is 64, `hw.cpufrequency` is 64 on some releases and not on
/// others), so the width is taken from what came back rather than assumed.
fn sysctl_integer(name: &CStr) -> Option<u64> {
    let bytes = sysctl_bytes(name)?;
    match bytes.len() {
        8 => Some(u64::from_ne_bytes(bytes.try_into().ok()?)),
        4 => Some(u32::from_ne_bytes(bytes.try_into().ok()?) as u64),
        _ => None,
    }
}

/// A string `sysctl`, without its terminating NUL.
fn sysctl_text(name: &CStr) -> Option<String> {
    let bytes = sysctl_bytes(name)?;
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[..end]).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// `(total, free)` physical memory in bytes.
pub(super) fn memory() -> Option<(f64, f64)> {
    let total = sysctl_integer(c"hw.memsize")? as f64;
    // SAFETY: `vm_statistics64` is plain integers, so all-zero is a valid value
    // for the kernel to overwrite.
    let mut info: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: `info` is a `vm_statistics64` and `count` is its length in
    // `integer_t` words — the pair `HOST_VM_INFO64` requires.
    let status = unsafe {
        libc::host_statistics64(
            host_self(),
            libc::HOST_VM_INFO64,
            (&mut info as *mut libc::vm_statistics64).cast(),
            &mut count,
        )
    };
    if status != libc::KERN_SUCCESS {
        return None;
    }
    let page = match unsafe { libc::sysconf(libc::_SC_PAGESIZE) } {
        held if held > 0 => held as f64,
        _ => return None,
    };
    // Read by value: the struct is `packed(8)`, so a reference to the field is
    // not allowed, and a copy is all this needs.
    let free_pages = info.free_count;
    Some((total, free_pages as f64 * page))
}

/// Seconds since boot.
pub(super) fn uptime() -> Option<f64> {
    let bytes = sysctl_bytes(c"kern.boottime")?;
    if bytes.len() < std::mem::size_of::<libc::timeval>() {
        return None;
    }
    // SAFETY: the kernel wrote a `timeval` — the length was checked above —
    // and `read_unaligned` makes no claim about the `Vec`'s alignment.
    let booted: libc::timeval = unsafe { bytes.as_ptr().cast::<libc::timeval>().read_unaligned() };
    // SAFETY: `time` with a null out-pointer only returns the current time.
    let now = unsafe { libc::time(null_mut()) };
    let seconds = now - booted.tv_sec;
    (seconds > 0).then_some(seconds as f64)
}

/// Every logical core, with its tick counters in milliseconds.
pub(super) fn cpus() -> Vec<Cpu> {
    let ticks_per_second = match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
        held if held > 0 => held as f64,
        _ => 100.0,
    };
    let ms = |ticks: u32| ticks as f64 * 1000.0 / ticks_per_second;
    let model = sysctl_text(c"machdep.cpu.brand_string").unwrap_or_else(|| "unknown".to_owned());
    let speed = sysctl_integer(c"hw.cpufrequency").map_or(0.0, |hertz| (hertz / 1_000_000) as f64);

    let mut count: libc::natural_t = 0;
    let mut records: libc::processor_info_array_t = null_mut();
    let mut words: libc::mach_msg_type_number_t = 0;
    // SAFETY: three valid out-pointers. On success the kernel maps an array of
    // `count` `processor_cpu_load_info` records into this task, `words`
    // `integer_t`s long, which is deallocated below.
    let status = unsafe {
        libc::host_processor_info(
            host_self(),
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut count,
            &mut records,
            &mut words,
        )
    };
    if status != libc::KERN_SUCCESS || records.is_null() {
        return Vec::new();
    }
    // SAFETY: `PROCESSOR_CPU_LOAD_INFO` answers exactly `count` records of this
    // type, contiguous, in the region the call just mapped.
    let loads = unsafe {
        std::slice::from_raw_parts(records.cast::<libc::processor_cpu_load_info>(), count as usize)
    };
    let held = loads
        .iter()
        .map(|load| {
            let at = |state: libc::c_int| ms(load.cpu_ticks[state as usize]);
            Cpu {
                model: model.clone(),
                speed,
                user: at(libc::CPU_STATE_USER),
                nice: at(libc::CPU_STATE_NICE),
                sys: at(libc::CPU_STATE_SYSTEM),
                idle: at(libc::CPU_STATE_IDLE),
                irq: 0.0,
            }
        })
        .collect();
    // SAFETY: the region `host_processor_info` mapped, at its own address and
    // length, released once and never read again — `loads` is not used past
    // the `collect` above.
    unsafe {
        libc::vm_deallocate(
            task_self(),
            records as libc::vm_address_t,
            words as usize * std::mem::size_of::<libc::integer_t>(),
        );
    }
    held
}

/// This host's Mach port, which the two host queries above are addressed to.
///
/// `libc` marks `mach_host_self` and `mach_task_self` deprecated in favour of
/// the `mach2` crate, which this crate does not take for three calls; the
/// symbols underneath are the stable Mach ABI every darwin program links, so
/// the allowance is scoped to these two one-line functions rather than the
/// module.
#[allow(deprecated)]
fn host_self() -> libc::mach_port_t {
    // SAFETY: no arguments and no preconditions.
    unsafe { libc::mach_host_self() }
}

/// This task's Mach port, which `vm_deallocate` names the address space by.
#[allow(deprecated)]
fn task_self() -> libc::mach_port_t {
    // SAFETY: reads the task-self port the loader initialised before `main`.
    unsafe { libc::mach_task_self() }
}
