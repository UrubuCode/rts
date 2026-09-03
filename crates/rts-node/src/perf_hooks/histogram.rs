//! `createHistogram`/`RecordableHistogram` and
//! `monitorEventLoopDelay`/`IntervalHistogram`.
//!
//! # Why the statistics are data properties, and why that is exact rather than
//! a snapshot
//!
//! Node's `count`/`min`/`max`/`mean`/`stddev`/`exceeds` are getters. This host
//! surface cannot define a named getter — `entry::define_getter` takes a
//! key-registry index no host module can mint — so they are ordinary data
//! properties, rewritten by every operation that can change them.
//!
//! For a `RecordableHistogram` that is not an approximation but an identity:
//! the only things that change a recordable histogram are `record`,
//! `recordDelta`, `add` and `reset`, and all four are natives here. For an
//! `IntervalHistogram` it holds too, because the sampling happens on the
//! program's own thread in [`source`] — the same pass that takes a sample
//! writes the properties, so a reading is never behind its own samples.
//!
//! # Why the sampler is the loop pump and not a background thread
//!
//! The reference doc (§5.3) proposes a dedicated `std::thread` that sleeps and
//! measures its own wake-up jitter, and names it a documented approximation:
//! it measures OS scheduling on an idle thread, not event-loop delay. The loop
//! source measures the real thing — how late a pass that was due at time `t`
//! actually happened, which is exactly "how long the loop was busy". It also
//! removes the background thread's two costs: a `Handle` table to keep the
//! thread's target alive, and a value written from a thread that does not own
//! the region.
//!
//! What it costs, named: samples accrue only while something else keeps the
//! loop running, because [`source`] answers [`Pending::Idle`] and an enabled
//! histogram must not hold a program open (Node's does not either). A program
//! that enables one and then does nothing asynchronous records nothing — which
//! is also the true answer, since a loop with nothing pending has no delay.
//!
//! # Not implemented, by name
//!
//! `percentile`, `percentileBigInt`, `percentiles`, `percentilesBigInt` — a
//! percentile needs the recorded distribution, and the running aggregates
//! below deliberately do not keep one; the plural form additionally answers a
//! `Map`, which nothing on this host surface can construct. Refused rather
//! than answered from a bucket table this module has not been asked to justify
//! against a measured need. `histogram[Symbol.dispose]` — no symbol-keyed
//! property can be installed from here.

use rts_core::entry::{self, Context, Pending, Provided};
use std::cell::RefCell;

/// The largest value a Node histogram tracks — one hour in nanoseconds. Past
/// it, a sample increments `exceeds` instead of joining the distribution.
const CEILING: f64 = 3_600_000_000_000.0;

/// The running aggregates every statistic below is derived from.
///
/// Sum and sum-of-squares rather than the samples: `mean` and `stddev` are
/// exact from them, they cost eight numbers regardless of how many samples are
/// recorded, and keeping the samples would be an unbounded allocation for the
/// one statistic (`percentile`) this module refuses anyway.
#[derive(Clone, Copy, Default)]
struct Stats {
    count: f64,
    min: f64,
    max: f64,
    sum: f64,
    squares: f64,
    exceeds: f64,
}

/// Where each aggregate lives on the instance.
///
/// The internal three are visible to a program, which Node's are not. Stated
/// rather than hidden: there is no non-enumerable property and no side table
/// keyed by object identity on this host surface, and a side table would in
/// any case have to be told when the object dies, which nothing here is told.
const SUM: &str = "__sum__";
const SQUARES: &str = "__squares__";
const LAST: &str = "__last__";

fn read(context: &mut Context, instance: u64) -> Stats {
    let number = |context: &mut Context, name: &str| {
        entry::number_of(entry::get_member(context, instance, name)).unwrap_or(0.0)
    };
    Stats {
        count: number(context, "count"),
        min: number(context, "min"),
        max: number(context, "max"),
        sum: number(context, SUM),
        squares: number(context, SQUARES),
        exceeds: number(context, "exceeds"),
    }
}

/// Writes every derived statistic back, in both the `number` and the `bigint`
/// spelling.
///
/// One function rather than a write at each mutation site: `count` and
/// `countBigInt` disagreeing is the class of defect that comes from two places
/// deciding one number.
fn write(context: &mut Context, instance: u64, stats: Stats) {
    let mean = match stats.count > 0.0 {
        true => stats.sum / stats.count,
        false => 0.0,
    };
    let variance = match stats.count > 0.0 {
        true => (stats.squares / stats.count - mean * mean).max(0.0),
        false => 0.0,
    };
    let plain: &[(&str, f64)] = &[
        ("count", stats.count),
        // An empty histogram answers `min` 0, where Node answers its internal
        // `i64::MAX` sentinel. Zero is the divergence chosen because the
        // sentinel is not a measurement and a program printing it would print
        // a number nothing recorded.
        ("min", stats.min),
        ("max", stats.max),
        ("mean", mean),
        ("stddev", variance.sqrt()),
        ("exceeds", stats.exceeds),
        (SUM, stats.sum),
        (SQUARES, stats.squares),
    ];
    for (name, value) in plain {
        let held = entry::make_number(*value);
        entry::put_member(context, instance, name, held);
    }
    for (name, value) in [
        ("countBigInt", stats.count),
        ("minBigInt", stats.min),
        ("maxBigInt", stats.max),
        ("exceedsBigInt", stats.exceeds),
    ] {
        let held = entry::make_bigint(context, value as i64);
        entry::put_member(context, instance, name, held);
    }
}

/// Folds one sample in, or counts it as exceeding.
fn fold(stats: &mut Stats, value: f64) {
    if value > CEILING {
        stats.exceeds += 1.0;
        return;
    }
    stats.min = match stats.count > 0.0 {
        true => stats.min.min(value),
        false => value,
    };
    stats.max = stats.max.max(value);
    stats.count += 1.0;
    stats.sum += value;
    stats.squares += value * value;
}

const RECORDABLE: &[(&str, Provided)] = &[
    ("record", record),
    ("recordDelta", record_delta),
    ("add", add),
    ("reset", reset),
];

const INTERVAL: &[(&str, Provided)] = &[("enable", enable), ("disable", disable), ("reset", reset)];

pub(super) fn install(context: &mut Context, namespace: u64) {
    entry::make_prototype(context, "Histogram", &[]);
    entry::make_prototype(context, "RecordableHistogram", RECORDABLE);
    entry::make_prototype(context, "IntervalHistogram", INTERVAL);
    let create = entry::make_callable(context, create_histogram);
    entry::put_member(context, namespace, "createHistogram", create);
    let monitor = entry::make_callable(context, monitor_event_loop_delay);
    entry::put_member(context, namespace, "monitorEventLoopDelay", monitor);
    for name in ["Histogram", "RecordableHistogram", "IntervalHistogram"] {
        let prototype = entry::make_prototype(context, name, &[]);
        let constructor = entry::make_callable(context, illegal);
        entry::put_member(context, constructor, "prototype", prototype);
        // Back-link (`crate::stream::class_ctor`'s doc) — `illegal` refuses
        // `new` on THIS constructor, but `create_histogram`/
        // `monitor_event_loop_delay` build real instances against this same
        // name-keyed prototype, and those instances' `.constructor.name`
        // reads off it too.
        entry::declare_host_class(context, constructor, prototype, name, 0);
        entry::put_member(context, namespace, name, constructor);
    }
    entry::declare_loop_source(context, "node:perf_hooks.delay", source);
}

extern "C" fn illegal(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// `createHistogram([options])`.
///
/// `options.lowest`/`highest`/`figures` are accepted and **ignored**, because
/// the aggregates above have no resolution to configure — there are no buckets
/// for `figures` to widen. Ignoring them is honest here in a way it would not
/// be elsewhere: they configure precision, every statistic this histogram
/// answers is exact, and none of them can therefore be answered less well.
/// What they also do in Node — bound the recordable range — is not honoured,
/// and `CEILING` is the only bound.
extern "C" fn create_histogram(
    _e: u64,
    _this: u64,
    _options: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "RecordableHistogram", RECORDABLE);
        let instance = entry::make_instance(context, prototype);
        write(context, instance, Stats::default());
        instance
    })
}

/// `histogram.record(value)` — nanoseconds, or whatever unit the caller feeds
/// it.
///
/// A value that is not a positive finite number records nothing and answers
/// `undefined`, where Node throws a `RangeError`; refused rather than thrown
/// for the reason the crate-module doc gives.
extern "C" fn record(_e: u64, this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // `number_of` and not `text_of`/`to_boolean`: the question is whether a
    // NUMBER was given, and a coercion would record the length of a string.
    let Some(value) = entry::number_of(value) else {
        return entry::undefined_value();
    };
    record_value(this, value);
    entry::undefined_value()
}

/// Folds a sample into a histogram from Rust — what [`super::timerify`] and
/// [`source`] both call.
///
/// Takes no context and reaches for one, because both callers are outside a
/// borrow and neither has one to pass. A histogram that is `undefined` (the
/// common `timerify(fn)` with no options) records nothing.
pub(super) fn record_value(instance: u64, value: f64) {
    if !value.is_finite() || value <= 0.0 {
        return;
    }
    entry::with_runtime(|context| {
        if !entry::is_object(context, instance) {
            return;
        }
        let mut stats = read(context, instance);
        fold(&mut stats, value);
        write(context, instance, stats);
    });
}

/// `histogram.recordDelta()` — the nanoseconds since the previous call.
///
/// The first call records nothing and only establishes the baseline, which is
/// Node's own behaviour: there is no earlier instant for it to have measured
/// from.
extern "C" fn record_delta(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let now = super::now_ns();
    let previous = entry::with_runtime(|context| {
        let held = entry::number_of(entry::get_member(context, this, LAST));
        let stamp = entry::make_number(now);
        entry::put_member(context, this, LAST, stamp);
        held
    });
    if let Some(previous) = previous {
        record_value(this, now - previous);
    }
    entry::undefined_value()
}

/// `histogram.add(other)` — merges every sample recorded in `other`.
///
/// Exact for `count`/`sum`/`squares`/`min`/`max`/`exceeds`, and therefore for
/// `mean` and `stddev`, because those are functions of the aggregates rather
/// than of the samples.
extern "C" fn add(_e: u64, this: u64, other: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        if !entry::is_object(context, other) {
            return;
        }
        let source = read(context, other);
        let mut stats = read(context, this);
        if source.count > 0.0 {
            stats.min = match stats.count > 0.0 {
                true => stats.min.min(source.min),
                false => source.min,
            };
            stats.max = stats.max.max(source.max);
        }
        stats.count += source.count;
        stats.sum += source.sum;
        stats.squares += source.squares;
        stats.exceeds += source.exceeds;
        write(context, this, stats);
    });
    entry::undefined_value()
}

extern "C" fn reset(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| write(context, this, Stats::default()));
    entry::undefined_value()
}

/// One enabled `IntervalHistogram`: which object, how often, and when its next
/// sample is due — all in milliseconds on [`super::now_ms`]'s clock.
struct Sampler {
    object: u64,
    resolution: f64,
    due: f64,
}

thread_local! {
    static SAMPLING: RefCell<Vec<Sampler>> = const { RefCell::new(Vec::new()) };
}

/// `monitorEventLoopDelay([options])` — created **disabled**, per Node.
extern "C" fn monitor_event_loop_delay(
    _e: u64,
    _this: u64,
    options: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "IntervalHistogram", INTERVAL);
        let instance = entry::make_instance(context, prototype);
        write(context, instance, Stats::default());
        let resolution = entry::number_of(entry::get_member(context, options, "resolution"))
            .filter(|given| *given > 0.0)
            .unwrap_or(10.0);
        let held = entry::make_number(resolution);
        entry::put_member(context, instance, "__resolution__", held);
        instance
    })
}

/// `histogram.enable()` — `true` if it was not already sampling.
extern "C" fn enable(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let resolution = entry::with_runtime(|context| {
        entry::number_of(entry::get_member(context, this, "__resolution__")).unwrap_or(10.0)
    });
    let now = super::now_ms();
    let started = SAMPLING.with(|held| {
        let mut samplers = held.borrow_mut();
        if samplers.iter().any(|sampler| sampler.object == this) {
            return false;
        }
        samplers.push(Sampler {
            object: this,
            resolution,
            due: now + resolution,
        });
        true
    });
    entry::boolean_value(started)
}

/// `histogram.disable()` — `true` if it was sampling.
extern "C" fn disable(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let stopped = SAMPLING.with(|held| {
        let mut samplers = held.borrow_mut();
        let before = samplers.len();
        samplers.retain(|sampler| sampler.object != this);
        samplers.len() != before
    });
    entry::boolean_value(stopped)
}

/// Takes one sample per enabled histogram whose sample is due.
///
/// The sample is how LATE this pass is: a pass due at `t` that happens at
/// `t + d` means the loop was busy for `d`, which is the delay Node reports.
///
/// [`Pending::Idle`] rather than [`Pending::In`]: an enabled histogram must not
/// hold a program open — see the module doc.
pub(super) fn source() -> Pending {
    let now = super::now_ms();
    let due: Vec<(u64, f64)> = SAMPLING.with(|held| {
        let mut samplers = held.borrow_mut();
        samplers
            .iter_mut()
            .filter(|sampler| now >= sampler.due)
            .map(|sampler| {
                let late = now - sampler.due;
                sampler.due = now + sampler.resolution;
                (sampler.object, late)
            })
            .collect()
    });
    for (object, late) in due {
        // Nanoseconds, which is the unit every histogram here records in. A
        // pass exactly on time is a zero-length delay, and `record_value`
        // refuses zero — so an on-time loop records nothing rather than
        // recording a sample of zero, which is the one place this diverges
        // from Node's histogram and is named here.
        record_value(object, late * 1_000_000.0);
    }
    Pending::Idle
}
