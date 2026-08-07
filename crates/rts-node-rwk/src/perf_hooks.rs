//! `node:perf_hooks` — `performance.now()`/`timeOrigin`/`mark`/`measure`/the
//! timeline readers, as a plain namespace over a process-global entry list.
//!
//! # Reuse-check
//!
//! Nothing in `rts-cranelift` answers this — a performance-entry timeline is
//! runtime bookkeeping, not a machine capability, and the search in
//! `.claude/skills/reuse-check/SKILL.md` over `src/tags/`, `src/shape/`,
//! `src/abi/`, `src/probe/` turned up nothing that mints timeline entries or a
//! monotonic-clock reading. `rts-core-rwk::entry::Context` holds no field for
//! one either (checked against `entry/mod.rs`'s `Context` inventory). The
//! nearest existing thing is `process::uptime`'s `OnceLock<Instant>` origin in
//! `process.rs` — a different origin (process start, not this module's own
//! first read) for a different purpose, so it is not reused; this module
//! mints its own origin the same way, once, for the same reason `uptime` does.
//!
//! # What this module is
//!
//! A `Vec` of `{name, entryType, startTime, duration}` records behind a
//! `Mutex`, plus a monotonic clock. Every mark/measure is a plain `.ts`-shaped
//! object built fresh on read — nothing here is a native `Handle`, because
//! nothing here needs GC visibility: a `PerformanceEntry` carries only text
//! and numbers.
//!
//! # `options.detail` — not carried
//!
//! `performance.mark(name, { detail })`'s `detail` can be an arbitrary JS
//! value, and holding one in [`ENTRIES`] across calls would be the same
//! cross-call-`u64`-in-a-static shape `fs/watch.rs`'s module doc names as the
//! one assumption this crate adds beyond the rest of it (no dereference as a
//! raw address, read back only by the thread that stored it — true here too,
//! but avoidable). Avoided rather than repeated: every entry's `detail` here
//! is always `null`, named rather than silently dropped.
//!
//! # Not implemented, by name
//!
//! `PerformanceObserver` — refused because it needs an event loop. Real
//! Node delivers a `PerformanceObserver` callback asynchronously, batched,
//! independent of any call the program happens to make next; that needs a
//! pump point this engine does not have (see `timers.rs`'s module doc for the
//! same wall, hit again here). `PerformanceResourceTiming`/
//! `markResourceTiming`/`setResourceTimingBufferSize` — no `fetch` in this
//! crate to feed them. `'gc'`/`'http'`/`'http2'`/`'net'`/`'dns'`-type entries —
//! no collector hook, no owning modules. `eventLoopUtilization` — no event
//! loop to measure idle/active time against. `timerify` — wrapping an
//! arbitrary JS function needs a native closure with a captured environment
//! slot, and [`rts_core_rwk::entry::make_callable`] hands back a fixed
//! function pointer with none (the same gap `events.rs`'s `rawListeners` doc
//! names). `createHistogram`/`monitorEventLoopDelay`/`Histogram` family — a
//! real HDR histogram is a genuine data structure this module has not been
//! asked to justify against a measured need yet (`perf-claim` territory, not
//! `add-entry-point`). `nodeTiming` — no boot-milestone timestamps are
//! recorded anywhere this crate can read. `measure()`'s `options`-object form
//! (`{start, end, duration, detail}`) — only the two-string-argument form
//! (`measure(name, startMark, endMark)`) is implemented; a `measure` with an
//! options object reads its `start`/`end` as absent and measures from the
//! origin to now, which is the same "measure with nothing given" case rather
//! than a silent misread.

use rts_core_rwk::entry::{Context, Provided};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// One buffered timeline entry.
struct Entry {
    name: String,
    entry_type: &'static str,
    start_time: f64,
    duration: f64,
}

static ORIGIN: OnceLock<Instant> = OnceLock::new();
static ENTRIES: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

/// Milliseconds since this module's own origin — set on first read, which is
/// the same "first reach wins" rule [`super::process`]'s `uptime` uses, and
/// for the same reason: no earlier point exists for this crate to record.
fn now_ms() -> f64 {
    let origin = ORIGIN.get_or_init(Instant::now);
    origin.elapsed().as_secs_f64() * 1000.0
}

/// `performance.timeOrigin` — the origin's wall-clock reading, taken once at
/// the same instant [`now_ms`]'s origin is set, so the two never drift apart.
fn time_origin_ms() -> f64 {
    static WALL: OnceLock<f64> = OnceLock::new();
    *WALL.get_or_init(|| {
        ORIGIN.get_or_init(Instant::now);
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    })
}

const PERFORMANCE_METHODS: &[(&str, Provided)] = &[
    ("now", now),
    ("mark", mark),
    ("measure", measure),
    ("clearMarks", clear_marks),
    ("clearMeasures", clear_measures),
    ("getEntries", get_entries),
    ("getEntriesByName", get_entries_by_name),
    ("getEntriesByType", get_entries_by_type),
];

/// The namespace `node:perf_hooks` is — just `{ performance }`. See the
/// module doc's "Not implemented" section for the rest of the real surface.
pub fn namespace(context: &mut Context) -> u64 {
    let performance = rts_core_rwk::entry::make_namespace(context, PERFORMANCE_METHODS);
    let origin = rts_core_rwk::entry::make_number(time_origin_ms());
    rts_core_rwk::entry::put_member(context, performance, "timeOrigin", origin);
    let namespace = rts_core_rwk::entry::make_object(context);
    rts_core_rwk::entry::put_member(context, namespace, "performance", performance);
    namespace
}

extern "C" fn now(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::make_number(now_ms())
}

/// `performance.mark(name, options?)` — `options.startTime` is the one field
/// read; `options.detail` is not (see the module doc).
extern "C" fn mark(_e: u64, _this: u64, name: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(name_text) = text(name) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let start_time = start_time_option(options).unwrap_or_else(now_ms);
    let record = Entry { name: name_text, entry_type: "mark", start_time, duration: 0.0 };
    let built = build_entry(&record);
    push(record);
    built
}

/// `options.startTime`, if `options` is an object carrying one.
fn start_time_option(options: u64) -> Option<f64> {
    let absent = rts_core_rwk::entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = rts_core_rwk::entry::with_runtime(|context| {
        rts_core_rwk::entry::get_member(context, options, "startTime")
    });
    rts_core_rwk::entry::number_of(value)
}

/// `performance.measure(name, startMark?, endMark?)` — string mark names or
/// numeric timestamps for either bound; an options-object second argument is
/// not implemented (see the module doc).
extern "C" fn measure(_e: u64, _this: u64, name: u64, start: u64, end: u64, _a3: u64) -> u64 {
    let Some(name_text) = text(name) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let start_time = resolve_time(start).unwrap_or(0.0);
    let end_time = resolve_time(end).unwrap_or_else(now_ms);
    let record = Entry {
        name: name_text,
        entry_type: "measure",
        start_time,
        duration: (end_time - start_time).max(0.0),
    };
    let built = build_entry(&record);
    push(record);
    built
}

/// A `measure` bound: a number is a timestamp directly, a string is a mark or
/// measure name whose own `startTime` is looked up, absent is `None`.
fn resolve_time(value: u64) -> Option<f64> {
    let absent = rts_core_rwk::entry::undefined_value();
    if value == absent {
        return None;
    }
    if let Some(number) = rts_core_rwk::entry::number_of(value) {
        return Some(number);
    }
    let name = text(value)?;
    let entries = ENTRIES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    entries.iter().rev().find(|entry| entry.name == name).map(|entry| entry.start_time)
}

extern "C" fn clear_marks(_e: u64, _this: u64, name: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    clear("mark", text(name));
    rts_core_rwk::entry::undefined_value()
}

extern "C" fn clear_measures(_e: u64, _this: u64, name: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    clear("measure", text(name));
    rts_core_rwk::entry::undefined_value()
}

fn clear(entry_type: &str, name: Option<String>) {
    let mut entries = ENTRIES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    entries.retain(|entry| {
        entry.entry_type != entry_type || name.as_deref().is_some_and(|target| entry.name != target)
    });
}

extern "C" fn get_entries(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entries_array(|_| true)
}

extern "C" fn get_entries_by_name(_e: u64, _this: u64, name: u64, kind: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(name_text) = text(name) else {
        return rts_core_rwk::entry::make_array(Vec::new());
    };
    let kind_text = text(kind);
    entries_array(|entry| {
        entry.name == name_text && kind_text.as_deref().is_none_or(|kind| entry.entry_type == kind)
    })
}

extern "C" fn get_entries_by_type(_e: u64, _this: u64, kind: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(kind_text) = text(kind) else {
        return rts_core_rwk::entry::make_array(Vec::new());
    };
    entries_array(|entry| entry.entry_type == kind_text)
}

/// Every buffered entry matching `keep`, built into JS objects, oldest first
/// (the order [`push`] already maintains).
fn entries_array(keep: impl Fn(&Entry) -> bool) -> u64 {
    let entries = ENTRIES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let built: Vec<u64> = entries.iter().filter(|entry| keep(entry)).map(build_entry).collect();
    rts_core_rwk::entry::make_array(built)
}

fn push(record: Entry) {
    ENTRIES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(record);
}

/// A `{name, entryType, startTime, duration, detail: null}` object over one
/// record. `detail` is always `null` — see the module doc.
fn build_entry(record: &Entry) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let object = rts_core_rwk::entry::make_object(context);
        let name = rts_core_rwk::entry::make_string(context, &record.name);
        rts_core_rwk::entry::put_member(context, object, "name", name);
        let entry_type = rts_core_rwk::entry::make_string(context, record.entry_type);
        rts_core_rwk::entry::put_member(context, object, "entryType", entry_type);
        let start_time = rts_core_rwk::entry::make_number(record.start_time);
        rts_core_rwk::entry::put_member(context, object, "startTime", start_time);
        let duration = rts_core_rwk::entry::make_number(record.duration);
        rts_core_rwk::entry::put_member(context, object, "duration", duration);
        let detail = rts_core_rwk::entry::null_in(context);
        rts_core_rwk::entry::put_member(context, object, "detail", detail);
        object
    })
}

/// An argument as text, `None` for `undefined`/a non-string.
fn text(value: u64) -> Option<String> {
    let absent = rts_core_rwk::entry::undefined_value();
    match value == absent {
        true => None,
        false => rts_core_rwk::entry::text_of(value),
    }
}
