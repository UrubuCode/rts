//! node:perf_hooks — the performance timeline: a real high-resolution monotonic
//! clock (`now`/`timeOrigin`) plus a mark/measure entry store. Every timing is
//! measured from `std::time` — no fabricated values.

use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// A performance-timeline entry (`PerformanceMark` / `PerformanceMeasure`).
#[derive(Clone)]
pub struct PerfEntry {
    pub name: String,
    pub entry_type: &'static str,
    pub start_time: f64,
    pub duration: f64,
}

/// The shared clock origin: the `Instant` all `now()` readings subtract from,
/// paired with the wall-clock unix-milliseconds at that same moment
/// (`timeOrigin`). Captured once, on first access.
fn origin() -> (Instant, f64) {
    static ORIGIN: OnceLock<(Instant, f64)> = OnceLock::new();
    *ORIGIN.get_or_init(|| {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        (Instant::now(), unix_ms)
    })
}

/// `performance.now()` — high-resolution milliseconds since `timeOrigin`.
pub fn now() -> f64 {
    origin().0.elapsed().as_secs_f64() * 1000.0
}

/// `performance.timeOrigin` — unix-milliseconds when the clock origin was taken.
pub fn time_origin() -> f64 {
    origin().1
}

fn entries() -> &'static Mutex<Vec<PerfEntry>> {
    static ENTRIES: OnceLock<Mutex<Vec<PerfEntry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Records a `mark` entry at the current time; returns its `startTime`.
pub fn add_mark(name: &str) -> f64 {
    let start = now();
    entries().lock().unwrap().push(PerfEntry {
        name: name.to_string(),
        entry_type: "mark",
        start_time: start,
        duration: 0.0,
    });
    start
}

/// The `startTime` of the most recent mark called `name`, if any.
fn mark_time(name: &str) -> Option<f64> {
    entries()
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e.entry_type == "mark" && e.name == name)
        .map(|e| e.start_time)
}

/// Records a `measure`. `start_mark`/`end_mark` name earlier marks (empty =
/// origin / now respectively). Returns `(start_time, duration)`.
pub fn add_measure(name: &str, start_mark: &str, end_mark: &str) -> (f64, f64) {
    let start = if start_mark.is_empty() { 0.0 } else { mark_time(start_mark).unwrap_or(0.0) };
    let end = if end_mark.is_empty() { now() } else { mark_time(end_mark).unwrap_or_else(now) };
    let duration = end - start;
    entries().lock().unwrap().push(PerfEntry {
        name: name.to_string(),
        entry_type: "measure",
        start_time: start,
        duration,
    });
    (start, duration)
}

/// A snapshot of the timeline, oldest first (`getEntries`). With a non-empty
/// `by_name`/`by_type` filter, only matching entries.
pub fn snapshot(by_name: Option<&str>, by_type: Option<&str>) -> Vec<PerfEntry> {
    entries()
        .lock()
        .unwrap()
        .iter()
        .filter(|e| by_name.is_none_or(|n| e.name == n))
        .filter(|e| by_type.is_none_or(|t| e.entry_type == t))
        .cloned()
        .collect()
}

/// `clearMarks([name])` / `clearMeasures([name])`. Empty `name` clears all of
/// that type.
pub fn clear(entry_type: &'static str, name: &str) {
    entries()
        .lock()
        .unwrap()
        .retain(|e| !(e.entry_type == entry_type && (name.is_empty() || e.name == name)));
}
