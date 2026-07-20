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
//! Layout: `store` (clock + entry store), `symbols` (extern points), `mod`
//! (registration).

mod store;
mod symbols;

use rts_engine::AbiType::{self, F64, Handle, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn f(name: &str, symbol: &str, args: Vec<AbiType>, ret: AbiType, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
        emit: None,
    }
}

/// Registers the `node:perf_hooks` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:perf_hooks")
        .doc("Performance timeline (node:perf_hooks): now/timeOrigin, mark/measure, getEntries*, clearMarks/clearMeasures.")
        .member(f("now", "__RTS_FN_NODE_PERF_NOW", vec![], F64, "now(): number", s::__RTS_FN_NODE_PERF_NOW as *const u8))
        .member(f("timeOrigin", "__RTS_FN_NODE_PERF_TIME_ORIGIN", vec![], F64, "timeOrigin(): number", s::__RTS_FN_NODE_PERF_TIME_ORIGIN as *const u8))
        .member(f("mark", "__RTS_FN_NODE_PERF_MARK", vec![StrPtr], Handle, "mark(name: string): object", s::__RTS_FN_NODE_PERF_MARK as *const u8))
        .member(f("measure", "__RTS_FN_NODE_PERF_MEASURE", vec![StrPtr], Handle, "measure(name: string): object", s::__RTS_FN_NODE_PERF_MEASURE as *const u8))
        .member(f("measure", "__RTS_FN_NODE_PERF_MEASURE_MARKS", vec![StrPtr, StrPtr, StrPtr], Handle, "measure(name: string, startMark: string, endMark: string): object", s::__RTS_FN_NODE_PERF_MEASURE_MARKS as *const u8))
        .member(f("getEntries", "__RTS_FN_NODE_PERF_GET_ENTRIES", vec![], Handle, "getEntries(): object[]", s::__RTS_FN_NODE_PERF_GET_ENTRIES as *const u8))
        .member(f("getEntriesByName", "__RTS_FN_NODE_PERF_GET_ENTRIES_BY_NAME", vec![StrPtr], Handle, "getEntriesByName(name: string): object[]", s::__RTS_FN_NODE_PERF_GET_ENTRIES_BY_NAME as *const u8))
        .member(f("getEntriesByType", "__RTS_FN_NODE_PERF_GET_ENTRIES_BY_TYPE", vec![StrPtr], Handle, "getEntriesByType(type: string): object[]", s::__RTS_FN_NODE_PERF_GET_ENTRIES_BY_TYPE as *const u8))
        .member(f("clearMarks", "__RTS_FN_NODE_PERF_CLEAR_MARKS_ALL", vec![], Void, "clearMarks(): void", s::__RTS_FN_NODE_PERF_CLEAR_MARKS_ALL as *const u8))
        .member(f("clearMarks", "__RTS_FN_NODE_PERF_CLEAR_MARKS", vec![StrPtr], Void, "clearMarks(name: string): void", s::__RTS_FN_NODE_PERF_CLEAR_MARKS as *const u8))
        .member(f("clearMeasures", "__RTS_FN_NODE_PERF_CLEAR_MEASURES_ALL", vec![], Void, "clearMeasures(): void", s::__RTS_FN_NODE_PERF_CLEAR_MEASURES_ALL as *const u8))
        .member(f("clearMeasures", "__RTS_FN_NODE_PERF_CLEAR_MEASURES", vec![StrPtr], Void, "clearMeasures(name: string): void", s::__RTS_FN_NODE_PERF_CLEAR_MEASURES as *const u8))
        .done();
}
