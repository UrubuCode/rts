//! `node:perf_hooks` — the performance measurement timeline. The
//! stream/async-independent core: a real high-resolution monotonic clock
//! (`now`/`timeOrigin`) and a mark/measure entry store (`mark`, `measure`,
//! `getEntries`/`getEntriesByName`/`getEntriesByType`, `clearMarks`/
//! `clearMeasures`). Every timing is measured from `std::time` — no fabricated
//! values.
//!
//! Node nests these under a `performance` object; RTS surfaces them as module
//! functions (`import { now, mark, measure } from "node:perf_hooks"`) — values
//! genuine, access is a call.
//!
//! Deferred (need the event-loop / async / observer subsystems): the
//! `PerformanceObserver` class + `observe`, `eventLoopUtilization`,
//! `monitorEventLoopDelay` (the histogram), `createHistogram`, the
//! `PerformanceResourceTiming`/GC entry sources, `markResourceTiming`.
//!
//! Layout: `store` (clock + entry store), `symbols` (`#[rtse::function]`
//! entry points), `mod` (registration).

mod store;
mod symbols;

use rts_engine::Engine;

/// Registers the `node:perf_hooks` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:perf_hooks")
        .doc("Performance timeline (node:perf_hooks): now/timeOrigin, mark/measure, getEntries*, clearMarks/clearMeasures.")
        .member(s::now_entry())
        .member(s::time_origin_entry())
        .member(s::mark_entry())
        .member(s::measure_entry())
        .member(s::measure_marks_entry())
        .member(s::get_entries_entry())
        .member(s::get_entries_by_name_entry())
        .member(s::get_entries_by_type_entry())
        .member(s::clear_marks_all_entry())
        .member(s::clear_marks_entry())
        .member(s::clear_measures_all_entry())
        .member(s::clear_measures_entry())
        .done();
}
