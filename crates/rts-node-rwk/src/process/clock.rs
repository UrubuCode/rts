//! Elapsed time and CPU time.
//!
//! # One reference point, shared
//!
//! `uptime()` and `hrtime()` both measure from [`STARTED`], captured the first
//! time this module is reached — which is namespace construction, before the
//! program's first statement. Two `Instant`s would make
//! `hrtime()[0] - uptime()` a number with no meaning, and Node's own guarantee
//! for `hrtime` is only that it is monotonic against an unspecified origin, so
//! sharing one is both cheaper and more truthful than inventing a second.

use rts_core_rwk::entry;
use std::sync::OnceLock;
use std::time::Instant;

/// The origin both clocks measure from.
static STARTED: OnceLock<Instant> = OnceLock::new();

/// That origin, capturing it if this is the first read.
fn started() -> Instant {
    *STARTED.get_or_init(Instant::now)
}

/// Puts `hrtime` (with its `bigint` member) on the namespace, and fixes the
/// clock origin at construction time.
pub(super) fn install(context: &mut entry::Context, namespace: u64) {
    started();
    // `hrtime` is a function that also carries a function: `process.hrtime()`
    // and `process.hrtime.bigint()` are both written by real programs. A
    // callable is an ordinary cell here, so a member hangs on it the same way
    // `node:events` hangs `prototype` on its constructor.
    let hrtime = entry::make_callable(context, hrtime_call);
    let bigint = entry::make_callable(context, hrtime_bigint);
    entry::put_member(context, hrtime, "bigint", bigint);
    entry::put_member(context, namespace, "hrtime", hrtime);
}

/// `process.uptime()` — seconds since [`STARTED`].
pub(super) extern "C" fn uptime(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(started().elapsed().as_secs_f64())
}

/// `process.hrtime([previous])` — `[seconds, nanoseconds]`.
///
/// With an argument, the DIFFERENCE from it, which is the only form Node
/// documents as meaningful. The argument is accepted only when it really is an
/// array: a number or a string there would otherwise read as `[0, 0]` and turn
/// a diff into an absolute reading that looks plausible.
pub(super) extern "C" fn hrtime_call(
    _e: u64,
    _this: u64,
    previous: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let elapsed = started().elapsed();
    let mut seconds = elapsed.as_secs() as i64;
    let mut nanos = i64::from(elapsed.subsec_nanos());
    if entry::is_array(previous) {
        let (was_seconds, was_nanos) = pair_of(previous);
        let total = (seconds - was_seconds) * 1_000_000_000 + (nanos - was_nanos);
        seconds = total.div_euclid(1_000_000_000);
        nanos = total.rem_euclid(1_000_000_000);
    }
    entry::with_runtime(|context| {
        let values = vec![
            entry::make_number(seconds as f64),
            entry::make_number(nanos as f64),
        ];
        entry::make_array_in(context, values)
    })
}

/// `process.hrtime.bigint()` — the same clock as one nanosecond count.
///
/// A real `BigInt`, through [`entry::make_bigint`]. This module's predecessor
/// refused the member because nothing on the host surface could construct one;
/// `node:sqlite` hit the same wall, the constructor was added, and the refusal
/// is withdrawn rather than left standing in a doc nobody rechecked.
pub(super) extern "C" fn hrtime_bigint(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    // `u128` nanoseconds narrowed to `i64`: 292 years of uptime before that
    // saturates, which is longer than the process.
    let nanos = started().elapsed().as_nanos().min(i64::MAX as u128) as i64;
    entry::with_runtime(|context| entry::make_bigint(context, nanos))
}

/// Two numeric elements of an array, as integers.
fn pair_of(value: u64) -> (i64, i64) {
    let at = |index: f64| {
        let element = entry::get_indexed(value, entry::make_number(index));
        entry::number_of(element).unwrap_or(0.0) as i64
    };
    (at(0.0), at(1.0))
}

/// `process.cpuUsage([previous])` — `{ user, system }` in microseconds.
///
/// Cross-platform: POSIX `getrusage`, Windows `GetProcessTimes` — see
/// [`process_time`]. `getrusage` reports the whole process, matching Node —
/// `threadCpuUsage` is a different call and is refused rather than aliased to
/// this one, because answering process time under a thread's name is the
/// wrong-answer shape this crate refuses.
pub(super) extern "C" fn cpu_usage(
    _e: u64,
    _this: u64,
    previous: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some((mut user, mut system)) = process_time() else {
        eprintln!("rts: process.cpuUsage: reading process CPU time failed");
        return entry::undefined_value();
    };
    // Read under ONE borrow: `get_member` takes a context, so a second pass
    // would be a second borrow for no reason. `number_of` is pure.
    let (was_user, was_system) = entry::with_runtime(|context| {
        (
            entry::number_of(entry::get_member(context, previous, "user")),
            entry::number_of(entry::get_member(context, previous, "system")),
        )
    });
    if let (Some(before_user), Some(before_system)) = (was_user, was_system) {
        user -= before_user;
        system -= before_system;
    }
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "user", entry::make_number(user));
        entry::put_member(context, object, "system", entry::make_number(system));
        object
    })
}

/// User and system CPU microseconds for this process, or `None` if the OS
/// refused to say — which is answered as `undefined` rather than as zero,
/// because zero is a plausible reading and "no reading" is not.
#[cfg(unix)]
fn process_time() -> Option<(f64, f64)> {
    // SAFETY: `getrusage` writes a `rusage` it is given a pointer to and reads
    // nothing else; the zeroed value is a valid `rusage` for it to overwrite.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let micros = |time: libc::timeval| time.tv_sec as f64 * 1e6 + time.tv_usec as f64;
    Some((micros(usage.ru_utime), micros(usage.ru_stime)))
}

/// Windows form of [`process_time`] — `GetProcessTimes`, the same
/// `extern "system"` raw-FFI shape `crate::os` already uses for Win32 calls
/// with no crate wrapping them (`docs/reference/node/crates.md` §2 accepts
/// this pattern; no new dependency is added here).
#[cfg(windows)]
fn process_time() -> Option<(f64, f64)> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    // FILETIME is in 100-nanosecond ticks; /10 gives microseconds.
    let ticks = |time: FileTime| ((u64::from(time.high) << 32) | u64::from(time.low)) as f64 / 10.0;
    let zero = FileTime { low: 0, high: 0 };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: each pointer targets a local `FileTime` the call fills in;
    // `GetCurrentProcess` is a pseudo-handle that needs no closing.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user) };
    (ok != 0).then_some((ticks(user), ticks(kernel)))
}

#[cfg(not(any(unix, windows)))]
fn process_time() -> Option<(f64, f64)> {
    None
}

/// `process.resourceUsage()` — the POSIX `rusage` struct, per
/// `docs/reference/node/process.md`. `userCPUTime`/`systemCPUTime` (µs) and
/// `maxRSS` (KB) are real, cross-platform (the same [`process_time`] plus
/// [`rss_kb`] `process.memoryUsage` also uses); the remaining `rusage`
/// fields (`sharedMemorySize`, page-fault/context-switch/IPC counters, …)
/// have no cross-platform source here — `getrusage` reports them on POSIX,
/// but `GetProcessMemoryInfo`/`GetProcessTimes` do not, and reporting a real
/// number on one platform and a fabricated `0` on the other under the same
/// name is the wrong-answer shape this crate refuses, so all of them are `0`
/// on every platform (honest "not tracked", not a Windows-only gap).
pub(super) extern "C" fn resource_usage(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let (user, system) = process_time().unwrap_or((0.0, 0.0));
    let max_rss_kb = rss_kb();
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let zero = entry::make_number(0.0);
        let fields: &[(&str, u64)] = &[
            ("userCPUTime", entry::make_number(user)),
            ("systemCPUTime", entry::make_number(system)),
            ("maxRSS", entry::make_number(max_rss_kb)),
            ("sharedMemorySize", zero),
            ("unsharedDataSize", zero),
            ("unsharedStackSize", zero),
            ("minorPageFault", zero),
            ("majorPageFault", zero),
            ("swappedOut", zero),
            ("fsRead", zero),
            ("fsWrite", zero),
            ("ipcSent", zero),
            ("ipcReceived", zero),
            ("signalsCount", zero),
            ("voluntaryContextSwitches", zero),
            ("involuntaryContextSwitches", zero),
        ];
        for (name, value) in fields {
            entry::put_member(context, object, name, *value);
        }
        object
    })
}

/// `process.memoryUsage()` — `{ rss, heapTotal, heapUsed, external,
/// arrayBuffers }`. `rss` is a real OS reading ([`rss_kb`] × 1024); the V8-
/// heap breakdown fields are `0` — this engine has no V8 heap and no
/// separate JS-heap byte accounting to report under those names, so `0` is
/// the honest "not tracked" answer rather than a fabricated split of `rss`.
pub(super) extern "C" fn memory_usage(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let rss = rss_kb() * 1024.0;
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let zero = entry::make_number(0.0);
        entry::put_member(context, object, "rss", entry::make_number(rss));
        entry::put_member(context, object, "heapTotal", zero);
        entry::put_member(context, object, "heapUsed", zero);
        entry::put_member(context, object, "external", zero);
        entry::put_member(context, object, "arrayBuffers", zero);
        object
    })
}

/// Resident-set size in KiB, `0` if the OS refuses to say.
#[cfg(unix)]
fn rss_kb() -> f64 {
    // SAFETY: same call/zeroing discipline as [`process_time`]'s POSIX form.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0.0;
    }
    // `ru_maxrss` is KB on Linux, bytes on macOS.
    let max = usage.ru_maxrss as f64;
    if cfg!(target_os = "macos") { max / 1024.0 } else { max }
}

/// Windows form of [`rss_kb`] — `K32GetProcessMemoryInfo`'s
/// `WorkingSetSize`, the closest cross-platform analogue of `rss` Windows
/// reports (Node's own `libuv` backend uses the same call).
#[cfg(windows)]
fn rss_kb() -> f64 {
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
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
    }
    // SAFETY: `counters` is a local struct sized correctly in `cb` before the
    // call, which is what `K32GetProcessMemoryInfo` requires.
    let mut counters: ProcessMemoryCounters = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok != 0 { counters.working_set_size as f64 / 1024.0 } else { 0.0 }
}

#[cfg(not(any(unix, windows)))]
fn rss_kb() -> f64 {
    0.0
}

/// `process.availableMemory()` — free physical memory in bytes, reusing
/// `node:os`'s own probe (`crate::os::machine::memory`) rather than a second
/// one: same number, same reuse-check outcome the old engine's
/// `crates/rts-node/src/process/metrics.rs` already reached for
/// `GlobalMemoryStatusEx`/`/proc/meminfo`.
pub(super) extern "C" fn available_memory(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let free = crate::os::machine::memory().map_or(0.0, |(_, free)| free);
    entry::make_number(free)
}

/// `process.constrainedMemory()` — the cgroup/job memory limit, or `0` when
/// the process is unconstrained. `0` unconditionally: nothing here reads a
/// cgroup v1/v2 file or a Windows Job Object limit, and `0` is Node's own
/// "unconstrained" answer, not a fabricated one.
pub(super) extern "C" fn constrained_memory(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(0.0)
}
