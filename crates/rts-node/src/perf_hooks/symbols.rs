//! node:perf_hooks — the `extern "C"` entry points over the timeline store.

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto, string_word};

use super::store::{self, PerfEntry};

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

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
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `performance.now()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_NOW() -> f64 {
    store::now()
}

/// `performance.timeOrigin`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_TIME_ORIGIN() -> f64 {
    store::time_origin()
}

/// `performance.mark(name)` → the mark entry.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_MARK(p: *const u8, l: i64) -> u64 {
    let name = read(p, l);
    let start = store::add_mark(&name);
    entry_obj(&PerfEntry { name, entry_type: "mark", start_time: start, duration: 0.0 })
}

/// `performance.measure(name)` — measures from origin to now.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_MEASURE(p: *const u8, l: i64) -> u64 {
    let name = read(p, l);
    let (start, dur) = store::add_measure(&name, "", "");
    entry_obj(&PerfEntry { name, entry_type: "measure", start_time: start, duration: dur })
}

/// `performance.measure(name, startMark, endMark)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_MEASURE_MARKS(
    p: *const u8,
    l: i64,
    sp: *const u8,
    sl: i64,
    ep: *const u8,
    el: i64,
) -> u64 {
    let name = read(p, l);
    let (start, dur) = store::add_measure(&name, &read(sp, sl), &read(ep, el));
    entry_obj(&PerfEntry { name, entry_type: "measure", start_time: start, duration: dur })
}

/// `performance.getEntries()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_GET_ENTRIES() -> u64 {
    entry_array(store::snapshot(None, None))
}

/// `performance.getEntriesByName(name)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_GET_ENTRIES_BY_NAME(p: *const u8, l: i64) -> u64 {
    let name = read(p, l);
    entry_array(store::snapshot(Some(&name), None))
}

/// `performance.getEntriesByType(type)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_GET_ENTRIES_BY_TYPE(p: *const u8, l: i64) -> u64 {
    let ty = read(p, l);
    entry_array(store::snapshot(None, Some(&ty)))
}

/// `performance.clearMarks([name])` — empty name clears all marks.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_CLEAR_MARKS(p: *const u8, l: i64) {
    store::clear("mark", &read(p, l));
}

/// `performance.clearMeasures([name])` — empty name clears all measures.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_CLEAR_MEASURES(p: *const u8, l: i64) {
    store::clear("measure", &read(p, l));
}

/// `performance.clearMarks()` — no argument → clear every mark.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_CLEAR_MARKS_ALL() {
    store::clear("mark", "");
}

/// `performance.clearMeasures()` — no argument → clear every measure.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PERF_CLEAR_MEASURES_ALL() {
    store::clear("measure", "");
}
