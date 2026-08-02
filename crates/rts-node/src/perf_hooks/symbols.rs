//! node:perf_hooks — the `#[rtse::function]` entry points over the timeline
//! store. Each function DECLARES its module and JS name; the linker symbol,
//! `AbiType`s, TS signature and doc are DERIVED from the Rust signature (see
//! docs/specs/rts-macro-single-source.md).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto, string_word};

use super::store::{self, PerfEntry};

fn num(v: f64) -> i64 {
    v.to_bits() as i64
}

/// One `PerformanceEntry` object → raw handle.
fn entry_obj(e: &PerfEntry) -> u64 {
    alloc_shaped_object(
        &["name", "entryType", "startTime", "duration"],
        &[
            string_word(e.name.as_bytes()) as i64,
            string_word(e.entry_type.as_bytes()) as i64,
            num(e.start_time),
            num(e.duration),
        ],
    )
}

/// A list of entries → array handle (elements boxed as OBJECT words).
fn entry_array(list: Vec<PerfEntry>) -> u64 {
    let words: Vec<i64> = list.iter().map(|e| handle_word_auto(entry_obj(e)) as i64).collect();
    alloc_entry(Entry::vec(words))
}

/// `performance.now()`.
#[rtse::function(module = "node:perf_hooks", value = "now")]
fn now() -> f64 {
    store::now()
}

/// `performance.timeOrigin`.
#[rtse::function(module = "node:perf_hooks", value = "timeOrigin")]
fn time_origin() -> f64 {
    store::time_origin()
}

/// `performance.mark(name)` → the mark entry.
#[rtse::function(module = "node:perf_hooks", value = "mark")]
fn mark(name: &str) -> Handle {
    let start = store::add_mark(name);
    entry_obj(&PerfEntry { name: name.to_string(), entry_type: "mark", start_time: start, duration: 0.0 })
}

/// `performance.measure(name)` — measures from origin to now.
#[rtse::function(module = "node:perf_hooks", value = "measure")]
fn measure(name: &str) -> Handle {
    let (start, dur) = store::add_measure(name, "", "");
    entry_obj(&PerfEntry { name: name.to_string(), entry_type: "measure", start_time: start, duration: dur })
}

/// `performance.measure(name, startMark, endMark)`.
#[rtse::function(module = "node:perf_hooks", value = "measure", overload = "marks")]
fn measure_marks(name: &str, start_mark: &str, end_mark: &str) -> Handle {
    let (start, dur) = store::add_measure(name, start_mark, end_mark);
    entry_obj(&PerfEntry { name: name.to_string(), entry_type: "measure", start_time: start, duration: dur })
}

/// `performance.getEntries()`.
#[rtse::function(module = "node:perf_hooks", value = "getEntries")]
fn get_entries() -> Handle {
    entry_array(store::snapshot(None, None))
}

/// `performance.getEntriesByName(name)`.
#[rtse::function(module = "node:perf_hooks", value = "getEntriesByName")]
fn get_entries_by_name(name: &str) -> Handle {
    entry_array(store::snapshot(Some(name), None))
}

/// `performance.getEntriesByType(type)`.
#[rtse::function(module = "node:perf_hooks", value = "getEntriesByType")]
fn get_entries_by_type(entry_type: &str) -> Handle {
    entry_array(store::snapshot(None, Some(entry_type)))
}

/// `performance.clearMarks()` — no argument → clear every mark.
#[rtse::function(module = "node:perf_hooks", value = "clearMarks")]
fn clear_marks_all() {
    store::clear("mark", "");
}

/// `performance.clearMarks([name])` — empty name clears all marks.
#[rtse::function(module = "node:perf_hooks", value = "clearMarks", overload = "name")]
fn clear_marks(name: &str) {
    store::clear("mark", name);
}

/// `performance.clearMeasures()` — no argument → clear every measure.
#[rtse::function(module = "node:perf_hooks", value = "clearMeasures")]
fn clear_measures_all() {
    store::clear("measure", "");
}

/// `performance.clearMeasures([name])` — empty name clears all measures.
#[rtse::function(module = "node:perf_hooks", value = "clearMeasures", overload = "name")]
fn clear_measures(name: &str) {
    store::clear("measure", name);
}
