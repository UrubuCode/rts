//! `node:perf_hooks` — the W3C User Timing timeline, `PerformanceObserver`,
//! `timerify`, and the recordable/interval histograms, over one monotonic
//! clock.
//!
//! # Reuse-check — what was searched, and what it found
//!
//! `rts-cranelift` answers none of this. `src/probe/` holds the only `Instant`
//! in that crate and it is a benchmark stopwatch rather than a service;
//! `src/tags/`, `src/shape/`, `src/abi/` and `src/types/` have nothing that
//! mints a timeline entry, a histogram or a sample. `src/sched/` owns the
//! microtask queue, which is *not* what observer delivery wants here — see
//! [`observer`] for why the loop source is the right seam and the microtask
//! queue is not reachable from a host module anyway.
//! `rts-core-rwk::entry::Context` (read in `entry/mod.rs`) holds no timing
//! field.
//!
//! Inside this crate the search found the pieces this module now reuses rather
//! than repeats: [`rts_core_rwk::entry::make_prototype`]'s name-keyed identity
//! (the same tool `async_hooks::attach` uses to give a class ONE prototype),
//! [`rts_core_rwk::entry::declare_loop_source`] (the one answer to "a module
//! with work left after the last statement", written because six modules had
//! six copies of it), and [`rts_core_rwk::entry::closure_new`] — which is what
//! makes `timerify` possible at all and was previously believed absent, see
//! [`timerify`].
//!
//! # The duplication that WAS real, and what is done about it here
//!
//! `rts-std-rwk/src/globals/timing.rs` builds a `performance` object too, and
//! its own module doc names the divergence: two objects, two unrelated
//! monotonic origins, and `globalThis.performance.now()` measuring from a
//! different instant than `require('node:perf_hooks').performance.now()`. It
//! also names the one-line fix available from *this* side, and this module now
//! takes it: the singleton is minted through
//! [`rts_core_rwk::entry::make_prototype`] under the name `"Performance"`,
//! which is idempotent by name, so whichever crate installs first mints the
//! cell and the other finds the SAME one. The members below are then installed
//! unconditionally (`make_prototype` installs its member list only on the call
//! that mints), so the object ends up carrying both crates' methods with ONE
//! `now`, ONE origin and ONE `timeOrigin`.
//!
//! `make_namespace` was what this module used before, and it lost because it
//! mints a fresh cell per call — which is what made the two objects two.
//!
//! The residue that cannot be removed from here: `rts-std-rwk`'s `ORIGIN`
//! static still exists and its `now`/`toJSON` are still written, they are just
//! overwritten on the shared cell by the ones here. Removing them is a change
//! in a crate this module does not own.
//!
//! # Why the timeline is per-thread and not process-global
//!
//! The reference doc (§5.4) asks for a process-wide buffer, and the previous
//! implementation had one — a `Mutex<Vec<..>>`. That was safe only because it
//! held nothing but `String`s and `f64`s. It cannot stay that way now that a
//! mark carries `detail`, which is an arbitrary JavaScript value belonging to
//! the region of the thread that made it: a `u64` handed to another thread
//! names a cell in a region that thread does not have. So the timeline is
//! `thread_local!`, which is also what Node actually does — a worker has its
//! own timeline — and the divergence the reference doc asked for is the one
//! being refused rather than introduced.
//!
//! # Not implemented, by name
//!
//! - **`performance.eventLoopUtilization` / the top-level
//!   `eventLoopUtilization`.** The reference doc (§5.2) suggests shipping a
//!   placeholder that reports `utilization ≈ 1.0`. That is refused: nothing in
//!   this engine measures idle time, so `1.0` is a number no clock produced,
//!   and a program branching on it would branch on a fabrication. `undefined`
//!   is the answer a program can test for.
//! - **`performance.nodeTiming` / `PerformanceNodeTiming`.** Every field is a
//!   boot milestone nothing in this engine records; `-1` for all of them would
//!   be Node's "not yet" sentinel used to mean "never measured", which reads
//!   as data.
//! - **`performance.markResourceTiming`, `setResourceTimingBufferSize`,
//!   `PerformanceResourceTiming`, `'resourcetimingbufferfull'`.** No `fetch`
//!   and no `http` in this crate to feed them. `clearResourceTimings` IS
//!   implemented, because "remove every `'resource'` entry" is a correct and
//!   complete answer over a timeline that has none.
//! - **`'gc'`/`'http'`/`'http2'`/`'net'`/`'dns'`/`'node'` entry types.** No
//!   collector hook and no owning modules. `PerformanceObserver` accepts these
//!   type names (Node does) and simply never delivers one; see
//!   [`observer::SUPPORTED`] for the set actually reported.
//! - **`PerformanceNodeEntry.flags` / `.kind`.** Meaningful only on a `'gc'`
//!   entry, which is never produced.
//! - **`histogram.percentile`/`percentileBigInt`/`percentiles`/
//!   `percentilesBigInt`.** A percentile needs the recorded distribution, not
//!   the running aggregates [`histogram`] keeps, and the `Map` the plural form
//!   answers cannot be constructed from this host surface at all — nothing on
//!   `rts_core_rwk::entry` mints one.
//! - **`RangeError` on invalid input** (`record(-1)`, `percentile(0)`,
//!   `createHistogram({figures: 9})`). Refused rather than approximated:
//!   `entry::throw` ends the program instead of reaching a `catch`, so a
//!   `assert.throws(...)` around one would abort the run rather than pass. The
//!   invalid value is not recorded and the call answers `undefined`.
//! - **`PerformanceObserver.observe` validation.** Node throws when both
//!   `type` and `entryTypes` are given, or neither. Same reason as above: the
//!   call subscribes to nothing and answers `undefined`.
//! - **`new PerformanceEntry()` / `new PerformanceMark()` / etc.** Node throws
//!   an illegal-constructor error; here they answer `undefined`. The
//!   constructors exist only so that `entry instanceof PerformanceMark` is
//!   true, which is what a program actually asks.
//! - **`histogram[Symbol.dispose]`.** No symbol-keyed property can be
//!   installed from this host surface (`put_member` interns a NAME).

mod histogram;
mod observer;
mod timeline;
mod timerify;

use rts_core_rwk::entry::{self, Context, Provided};
use std::sync::OnceLock;
use std::time::Instant;

/// The instant every reading here counts from.
///
/// Set on first read rather than at install, the same "first reach wins" rule
/// `node:process`'s `uptime` uses: `install` is not the earliest point in the
/// process, and pretending it is would put a number on `timeOrigin` no clock
/// produced.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// `performance.now()` — milliseconds since [`ORIGIN`], monotonic.
///
/// `Instant` and not `SystemTime`: a wall clock steps backwards when the system
/// time is adjusted, which would make a measured duration negative.
pub(crate) fn now_ms() -> f64 {
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// The same clock in nanoseconds, which is the unit every histogram here
/// records in (reference doc §4 — the unit mismatch inside this one module is
/// Node's, not an accident here).
///
/// `f64` rather than `u64` because every consumer either compares it or hands
/// it to `make_number`, and a `u64` past 2^53 would round on the way out
/// anyway; at nanosecond resolution that is 104 days of uptime, stated rather
/// than hidden.
pub(crate) fn now_ns() -> f64 {
    ORIGIN.get_or_init(Instant::now).elapsed().as_nanos() as f64
}

/// `performance.timeOrigin` — [`ORIGIN`]'s wall-clock reading in milliseconds
/// since the Unix epoch, taken once at the instant [`ORIGIN`] is established so
/// that `timeOrigin + now()` names a real wall-clock time.
fn time_origin_ms() -> f64 {
    static WALL: OnceLock<f64> = OnceLock::new();
    *WALL.get_or_init(|| {
        ORIGIN.get_or_init(Instant::now);
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    })
}

/// The members of the `performance` singleton. See the module doc for the rest
/// of the real surface, refused by name.
const PERFORMANCE_METHODS: &[(&str, Provided)] = &[
    ("now", now),
    ("toJSON", to_json),
    ("mark", timeline::mark),
    ("measure", timeline::measure),
    ("clearMarks", timeline::clear_marks),
    ("clearMeasures", timeline::clear_measures),
    ("clearResourceTimings", timeline::clear_resource_timings),
    ("getEntries", timeline::get_entries),
    ("getEntriesByName", timeline::get_entries_by_name),
    ("getEntriesByType", timeline::get_entries_by_type),
    ("timerify", timerify::timerify),
];

/// GC `detail.kind`/`detail.flags` values, as `perf_hooks.constants`.
///
/// Shipped even though no `'gc'` entry is ever produced, because these are
/// *constants a program compares against* rather than measurements — a program
/// writing `if (e.detail.kind === constants.NODE_PERFORMANCE_GC_MAJOR)` needs
/// the name to resolve, and the numbers are Node's own, not invented here.
const CONSTANTS: &[(&str, f64)] = &[
    ("NODE_PERFORMANCE_GC_MAJOR", 4.0),
    ("NODE_PERFORMANCE_GC_MINOR", 1.0),
    ("NODE_PERFORMANCE_GC_INCREMENTAL", 8.0),
    ("NODE_PERFORMANCE_GC_WEAKCB", 16.0),
    ("NODE_PERFORMANCE_GC_FLAGS_NO", 0.0),
    ("NODE_PERFORMANCE_GC_FLAGS_CONSTRUCT_RETAINED", 2.0),
    ("NODE_PERFORMANCE_GC_FLAGS_FORCED", 4.0),
    ("NODE_PERFORMANCE_GC_FLAGS_SYNCHRONOUS_PHANTOM_PROCESSING", 8.0),
    ("NODE_PERFORMANCE_GC_FLAGS_ALL_AVAILABLE_GARBAGE", 16.0),
    ("NODE_PERFORMANCE_GC_FLAGS_ALL_EXTERNAL_MEMORY", 32.0),
    ("NODE_PERFORMANCE_GC_FLAGS_SCHEDULE_IDLE", 64.0),
];

/// The namespace `node:perf_hooks` is.
pub fn namespace(context: &mut Context) -> u64 {
    // Idempotent by name, and that is the whole point — see the module doc's
    // duplication section. `make_prototype` installs its member list only on
    // the call that MINTS the cell, so the members are (re)installed below
    // whether this call or `rts-std-rwk`'s was the first.
    let performance = entry::make_prototype(context, "Performance", PERFORMANCE_METHODS);
    for (name, code) in PERFORMANCE_METHODS {
        let method = entry::make_callable(context, *code);
        entry::put_member(context, performance, name, method);
    }
    let origin = entry::make_number(time_origin_ms());
    // A data property rather than an accessor: `timeOrigin` is fixed for the
    // life of the process, and this host surface cannot define a named getter
    // anyway (`entry::define_getter` takes a key-registry index no host can
    // mint).
    entry::put_member(context, performance, "timeOrigin", origin);

    let namespace = entry::make_object(context);
    entry::put_member(context, namespace, "performance", performance);
    timeline::install_classes(context, namespace);
    observer::install(context, namespace);
    histogram::install(context, namespace);
    let timerify_fn = entry::make_callable(context, timerify::timerify);
    entry::put_member(context, namespace, "timerify", timerify_fn);

    let constants = entry::make_object(context);
    for (name, value) in CONSTANTS {
        let held = entry::make_number(*value);
        entry::put_member(context, constants, name, held);
    }
    entry::put_member(context, namespace, "constants", constants);

    // Registered by the module rather than by the host, which is the rule
    // `entry::declare_loop_source` was written to enforce: an observer whose
    // delivery depended on the host remembering to name this module would
    // deliver nothing.
    entry::declare_loop_source(context, "node:perf_hooks", observer::source);
    namespace
}

/// `performance.now()`.
///
/// Takes no borrow at all: a double is its own encoding rather than a cell, so
/// `make_number` needs no context.
extern "C" fn now(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(now_ms())
}

/// `performance.toJSON()` — `{ timeOrigin, now }`.
///
/// Node's answer also carries `nodeTiming`, and this one does not: every field
/// of that record is a boot milestone nothing here measures, so emitting the
/// key with zeros would be data a program could read and believe. Omitting it
/// is the difference a program can test for.
extern "C" fn to_json(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // Read BEFORE the borrow. Both are pure Rust, but reading them here keeps
    // the borrow body to allocation alone, which is the shape that makes an
    // accidental ambient call inside it visible.
    let origin = time_origin_ms();
    let current = now_ms();
    entry::with_runtime(|context| {
        let snapshot = entry::make_object(context);
        let origin = entry::make_number(origin);
        entry::put_member(context, snapshot, "timeOrigin", origin);
        let current = entry::make_number(current);
        entry::put_member(context, snapshot, "now", current);
        snapshot
    })
}
