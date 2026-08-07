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
/// POSIX only; see [`super`]'s refusal list for why there is no Windows form.
/// `getrusage` reports the whole process, matching Node — `threadCpuUsage` is
/// a different call and is refused rather than aliased to this one, because
/// answering process time under a thread's name is the wrong-answer shape this
/// crate refuses.
#[cfg(unix)]
pub(super) extern "C" fn cpu_usage(
    _e: u64,
    _this: u64,
    previous: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some((mut user, mut system)) = process_time() else {
        eprintln!("rts: process.cpuUsage: getrusage failed");
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
