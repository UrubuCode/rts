//! `performance` and `queueMicrotask` — the two standalone globals of
//! `docs/reference/node/globals.md` §2.12/§2.13 that this crate can honestly
//! supply, and the two it deliberately refuses to supply a **second** time.
//!
//! # Reuse-check — what was searched, and what it found
//!
//! `rts-cranelift` answers **one** of the four, and it is the one that matters
//! most. `src/sched/` owns the microtask queue (`Queues::wake`, the
//! `ContinuationId` space, the ordering), which is exactly what
//! `queueMicrotask` is a name for — so nothing here queues anything of its own.
//! `src/tags/`, `src/shape/`, `src/abi/` and `src/probe/` answer nothing about
//! a clock: `probe/harness.rs` holds the only `Instant` in that crate and it is
//! a benchmark stopwatch, not a service. `rts-core`'s `entry::Context`
//! (read in `entry/mod.rs`) holds no timing field either.
//!
//! Then the three names were searched across the workspace, and two of them
//! were **already answered**:
//!
//! - **`globalThis` — already provided; nothing is installed here.**
//!   `rts-core`'s `entry/global.rs` answers the name with the global object
//!   itself (its `global_get` match arm), and `rts-codegen`'s `emit/globals.rs`
//!   `PROVIDED` lists it, so the name already resolves. A second one would be a
//!   *different object* under the name whose whole point is being the one there
//!   is.
//! - **`structuredClone` — already provided; nothing is installed here.**
//!   `rts-core`'s `entry/clone.rs` is the walk *and* the global: its
//!   `provided()` hands `global_get` the native, and `emit/globals.rs`'s
//!   `PROVIDED` lists the name. `entry::deep_copy` is that same walk exported
//!   for a host that wants to call it from Rust — it is not an unwired
//!   capability waiting for a host to name it. Wrapping it here would put a
//!   second callable on the global object that **shadows** the engine's,
//!   because `declare_global` writes an own property and `global_get` reads own
//!   properties before it makes anything. One walk with two front doors, one of
//!   which is then unreachable, is the duplication `reuse-check` exists to
//!   stop.
//!
//! # The duplication that is real, named rather than hidden: `performance`
//!
//! `crates/rts-node/src/perf_hooks.rs` builds a `performance` object with
//! its own `OnceLock<Instant>` origin, and `rts-node`'s `install` already
//! declares it as a global (its `from_modules` list names
//! `("perf_hooks", &["performance"])`). So after
//! `crates/rts-host/src/run.rs` runs `rts_std::install` and then
//! `rts_node::install`, **node's object wins the name** — which is the
//! right outcome, because `globals.md` §2.13 requires
//! `globalThis.performance` to be the *same object identity* as
//! `require('node:perf_hooks').performance`.
//!
//! That leaves two objects with one behaviour and two unrelated monotonic
//! origins, and this crate cannot reach across to fix it: `rts-std`'s
//! manifest depends on `rts-core` alone, and depending on `rts-node`
//! would invert the layering — a web global cannot exist only when a Node
//! module was installed.
//!
//! What is done about it here is the half that can be done from this side: the
//! object is minted through [`rts_core::entry::make_prototype`], which is
//! **idempotent by name**, so `perf_hooks.rs` asking for `"Performance"` would
//! get *this* cell and hang `mark`/`measure`/`getEntries*` on it instead of
//! building a rival. That is a one-line change in a file this unit does not
//! own, and until it is made the divergence is: two origins, so
//! `performance.now()` and `require('node:perf_hooks').performance.now()`
//! measure from different instants. `make_namespace` was the alternative — it
//! is what `perf_hooks.rs` uses — and it lost because it mints a fresh cell per
//! call and therefore forecloses that fix entirely.
//!
//! # `queueMicrotask` — the queue is reached, not rebuilt
//!
//! There is no `enqueue_microtask` on the entry surface, and writing one here
//! would be a second answer to "what runs next". What the surface does export
//! is [`rts_core::entry::settled`], which hands back a promise that is
//! already fulfilled *through the machine's own scheduler*. Attaching the
//! callback with that promise's `then` records an ordinary reaction on
//! `rts_cranelift::sched::Queues`, and `entry/promise/state.rs`'s `react`
//! queues it rather than running it **even though the promise settled first**.
//! So the callback runs on the next drain, after the current synchronous code,
//! in the same one queue every other microtask uses — which is the whole
//! contract §2.12 states.
//!
//! A synchronous call under this name was the alternative. It is refused
//! outright: ordering is the entire specification of `queueMicrotask`, so a
//! synchronous call is not an approximation of it but a wrong answer that runs.
//!
//! # Not implemented, by name
//!
//! - `performance.mark`, `measure`, `clearMarks`, `clearMeasures`,
//!   `getEntries`, `getEntriesByName`, `getEntriesByType` — a timeline exists,
//!   in `node:perf_hooks`, and a second one here would be a second answer to
//!   what a program has measured. Reached by importing `node:perf_hooks`.
//! - `performance.nodeTiming` — no boot milestones are recorded anywhere this
//!   crate can read, so every field would be a number nothing measured.
//!   `toJSON` therefore omits it rather than emitting zeros.
//! - `performance.eventLoopUtilization` — needs idle/active accounting the
//!   loop (`entry::pump_sources`) does not keep.
//! - `performance.timerify` — needs a native closure over the wrapped
//!   function, and `entry::make_callable` hands back a bare function pointer
//!   with no environment slot.
//! - `performance.markResourceTiming`, `setResourceTimingBufferSize`,
//!   `clearResourceTimings`, `onresourcetimingbufferfull` — no `fetch` in this
//!   crate to feed them.
//! - `structuredClone`'s `options.transfer` — refused where the walk lives, for
//!   the reason `entry/clone.rs` records; nothing is added or subtracted here.
//!
//! Each of those reads as `undefined` on the object this module installs.
//!
//! # What this module cannot do from here, and the caller must
//!
//! `rts-codegen`'s `emit/globals.rs` `PROVIDED` list is what decides whether a
//! bare name is readable at all, and it lists **neither `performance` nor
//! `queueMicrotask`** today. A program writing `performance.now()` without
//! assigning the name first is refused at compile time with `UnboundName`, and
//! installing the value here does not change that — the two sets are
//! deliberately separate, as that module's own documentation states. Adding the
//! two names there is a change in a crate this unit does not own.

use rts_core::entry::{self, Context, Provided};

use std::sync::OnceLock;
use std::time::Instant;

/// The instant `now()` counts from.
///
/// Set on first read rather than at install: `install` is not the earliest
/// point in the process and pretending it is would put a number on
/// `timeOrigin` that no clock produced. The same "first reach wins" rule
/// `node:process`'s `uptime` and `perf_hooks.rs` both use — and the fact that
/// this is a *third* such origin in one process is the duplication the module
/// documentation names.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

/// `performance.now()` — milliseconds since [`ORIGIN`], monotonic.
///
/// `Instant` and not `SystemTime`: `now()` is defined to be monotonic, and a
/// wall clock steps backwards when the system time is adjusted, which would
/// make a duration computed from two readings negative. `Date.now()` is the
/// wall clock, and `entry/date/mod.rs` is where that one lives.
fn now_ms() -> f64 {
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// `performance.timeOrigin` — [`ORIGIN`]'s wall-clock reading in milliseconds
/// since the Unix epoch.
///
/// Taken once, at the instant [`ORIGIN`] is established, so the pair never
/// drifts: `timeOrigin + now()` has to name a real wall-clock time, and reading
/// the wall clock later would make that sum wrong by however long the two reads
/// were apart.
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

/// The members of the `performance` singleton. See the module documentation for
/// the rest of the real surface, refused by name.
const PERFORMANCE_METHODS: &[(&str, Provided)] = &[("now", now), ("toJSON", to_json)];

/// Installs `performance` and `queueMicrotask` as globals.
///
/// Globals rather than module members because a program writes
/// `performance.now()` and `queueMicrotask(f)` with no import line — the same
/// reason `console` is a global, recorded in `super::super::console`.
pub fn install(context: &mut Context) {
    // Idempotent by name, which is not a convenience here: it is the hook that
    // lets `node:perf_hooks` reach THIS object rather than build a second one.
    // See the module documentation.
    let performance = entry::make_prototype(context, "Performance", PERFORMANCE_METHODS);
    let origin = entry::make_number(time_origin_ms());
    // A data property rather than a getter. `timeOrigin` is fixed for the life
    // of the process, so a getter would be a call that answers a constant — and
    // `define_getter` costs an accessor record per read path for nothing.
    entry::put_member(context, performance, "timeOrigin", origin);
    entry::declare_global(context, "performance", performance);

    let queue = entry::make_callable(context, queue_microtask);
    entry::declare_global(context, "queueMicrotask", queue);
}

/// `performance.now()`.
///
/// Takes no borrow at all: a double is its own encoding rather than a cell, so
/// `make_number` needs no context — which is why the hottest member of this
/// object is also the cheapest.
extern "C" fn now(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(now_ms())
}

/// `performance.toJSON()` — `{ timeOrigin }`.
///
/// Node's answer also carries `nodeTiming`, and this one does not: every field
/// of that record is a boot milestone nothing in this engine measures, so
/// emitting the key with zeros in it would be data a program could read and
/// believe. Omitting the key is the difference a program can actually test for.
extern "C" fn to_json(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        // `make_object` and not the `object_new` entry point: the latter takes
        // the ambient borrow, and this body already holds one.
        let snapshot = entry::make_object(context);
        let origin = entry::make_number(time_origin_ms());
        entry::put_member(context, snapshot, "timeOrigin", origin);
        snapshot
    })
}

/// `queueMicrotask(callback)`.
///
/// # Why it goes through a settled promise
///
/// Because that is where the queue is. `entry::settled` fulfils a promise
/// through `rts_cranelift::sched`, and attaching a reaction to an
/// already-settled promise *queues* it — `entry/promise/state.rs`'s `react`
/// says so explicitly, and it is the property `Promise.resolve(1).then(f)` is
/// most tested on. So the callback lands in the one queue, in arrival order,
/// beside every other microtask, and no ordering is decided here.
///
/// # The borrow discipline, which is a process abort if it is wrong
///
/// `entry::call` is ambient: it takes a borrow of its own and then releases it
/// before jumping, because the callee's first act may be to call the runtime.
/// So the promise and its `then` are read under one borrow, the borrow is
/// dropped, and only then is the call made. A borrow held across it is a panic
/// in an `extern "C"` frame, which cannot unwind and therefore aborts.
///
/// # Divergences, by name
///
/// A non-callable argument is ignored and answers `undefined`; the language
/// throws a `TypeError`, which this engine cannot raise where a handler could
/// catch it — the same stated gap `entry::call` records for calling a
/// non-callable. The callback is reached through the fulfilled arm of a `then`,
/// so it arrives with one `undefined` argument where Node passes none —
/// invisible except to `arguments.length`, and not introduced here: the calling
/// convention pads every call to four slots, which `super::super::console`
/// records from the other side of the same fact. And an exception thrown inside
/// `callback` ends the program rather than surfacing as `process`'s
/// `'uncaughtException'`, because `entry::throw` ends the program everywhere;
/// the derived promise this discards is therefore never observed rejecting.
extern "C" fn queue_microtask(
    _e: u64,
    _this: u64,
    callback: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let prepared = entry::with_runtime(|context| {
        // Asked before anything is allocated: a `queueMicrotask(undefined)`
        // must not leave a promise behind whose only reaction is a value that
        // cannot be called.
        if !entry::is_callable_in(context, callback) {
            return None;
        }
        let absent = entry::undefined_in(context);
        let source = entry::settled(context, absent, false);
        // Off the prototype the registration linked, so a program that replaced
        // `Promise.prototype.then` is honoured here exactly as it would be at a
        // call site — reaching past it would make this the one `.then` in the
        // process nothing can intercept.
        let then_fn = entry::get_member(context, source, "then");
        Some((source, then_fn))
    });
    let absent = entry::undefined_value();
    let Some((source, then_fn)) = prepared else {
        return absent;
    };
    // No borrow held: `entry::call` takes its own, and the callback it will
    // eventually run is user code.
    entry::call(then_fn, source, callback, absent, absent, absent);
    absent
}
