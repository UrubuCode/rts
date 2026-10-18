//! `RTS_COMPILE_STATS=1`: how many functions a program COMPILES against how many it CALLS.
//!
//! A measurement aid for the lazy-compilation plan (`docs/codegen/lazy-compile-plan.md` §5,
//! stage 0), not a feature. Off (the env var unset) it costs one cached `OnceLock<bool>` read at
//! a few host seams and nothing at run time: the instrumentation is a call inserted into each
//! function ONLY when the flag is on, so a normal compile emits byte-identical code.
//!
//! # What is counted
//!
//! * **compiled** — every function the host places in executable memory
//!   (`Prepared.emitted.functions`): JS function bodies, async/generator wrappers, module
//!   entries, the script. Also reported: `rts_codegen::emit::JS_FUNCTIONS_EMITTED`, the number of
//!   JS function *bodies* the language layer emitted (excludes wrappers and module entries).
//! * **called** — distinct functions whose body ran at least once. Each compiled function gets
//!   `__rts_compile_stats_enter(<id>)` as its first instruction; the callee sets a bit. A
//!   function entered a million times counts once.
//!
//! `RTS_COMPILE_STATS=2` also lists the names of called and never-called functions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rts_cranelift::ir::{FuncId, Signature};
use rts_cranelift::repr::Repr;

use crate::run::Prepared;

/// The symbol the instrumented code calls.
pub(crate) const SYMBOL: &str = "__rts_compile_stats_enter";

struct Stats {
    names: Vec<String>,
    called: Vec<bool>,
    /// Per function: index into `units`, or `u32::MAX` when no unit's range holds it.
    unit: Vec<u32>,
    /// `(specifier, is_std)` per emitted unit.
    units: Vec<(String, bool)>,
    emit: Duration,
    prepare: Duration,
    place: Duration,
}

static STATS: Mutex<Stats> = Mutex::new(Stats {
    names: Vec::new(),
    called: Vec::new(),
    unit: Vec::new(),
    units: Vec::new(),
    emit: Duration::ZERO,
    prepare: Duration::ZERO,
    place: Duration::ZERO,
});
static START: OnceLock<Instant> = OnceLock::new();
static HOOKED: AtomicBool = AtomicBool::new(false);

/// Whether `RTS_COMPILE_STATS` is set (read once).
pub(crate) fn enabled() -> bool {
    static ASKED: OnceLock<bool> = OnceLock::new();
    *ASKED.get_or_init(|| std::env::var_os("RTS_COMPILE_STATS").is_some())
}

fn verbose() -> bool {
    std::env::var("RTS_COMPILE_STATS").is_ok_and(|v| v == "2")
}

/// Marks the start of the process's compilation work (wall clock origin).
pub(crate) fn note_start() {
    if enabled() {
        START.get_or_init(Instant::now);
    }
}

/// Which phase a duration belongs to.
pub(crate) enum Phase {
    Emit,
    Prepare,
    Place,
}

pub(crate) fn add_time(phase: Phase, took: Duration) {
    if !enabled() {
        return;
    }
    let mut stats = STATS.lock().unwrap_or_else(|e| e.into_inner());
    match phase {
        Phase::Emit => stats.emit += took,
        Phase::Prepare => stats.prepare += took,
        Phase::Place => stats.place += took,
    }
}

/// The function each instrumented body calls first.
pub(crate) extern "C" fn enter(id: i64) {
    if let Ok(mut stats) = STATS.lock()
        && let Some(slot) = stats.called.get_mut(id as usize)
    {
        *slot = true;
    }
}

/// Inserts the marker call into every compiled function and declares its callee.
pub(crate) fn instrument(prepared: &mut Prepared) {
    START.get_or_init(Instant::now);
    let shape = Signature {
        params: vec![Repr::I64],
        returns: vec![],
        convention: rts_cranelift::abi::Convention::Foreign,
        ..Signature::default()
    };
    let sig = prepared.funcs.declare_signature(shape);
    let callee = prepared.funcs.declare_function(sig);

    let named: std::collections::HashMap<FuncId, &str> = prepared
        .emitted
        .function_names
        .iter()
        .map(|(id, name, ..)| (*id, name.as_str()))
        .collect();
    let script = prepared.script;
    let mut labels = Vec::with_capacity(prepared.emitted.functions.len());
    for (id, _) in &prepared.emitted.functions {
        labels.push(match named.get(id) {
            _ if *id == script => "<script>".to_owned(),
            Some(name) if name.is_empty() => format!("<anonymous fn_{}>", id.index()),
            Some(name) => format!("{name} (fn_{})", id.index()),
            None => format!("<internal fn_{}>", id.index()),
        });
    }

    let ranges = std::mem::take(
        &mut *rts_codegen::emit::UNIT_FUNCTION_RANGES
            .lock()
            .unwrap_or_else(|e| e.into_inner()),
    );
    let mut stats = STATS.lock().unwrap_or_else(|e| e.into_inner());
    let unit_base = stats.units.len() as u32;
    for (specifier, is_std, _, _) in &ranges {
        stats.units.push((specifier.clone(), *is_std));
    }
    for (id, _) in &prepared.emitted.functions {
        let at = id.index();
        let unit = ranges
            .iter()
            .position(|(_, _, first, end)| (*first..*end).contains(&at))
            .map_or(u32::MAX, |p| unit_base + p as u32);
        stats.unit.push(unit);
    }
    let base = stats.names.len() as u64;
    let total = stats.names.len() + labels.len();
    stats.called.resize(total, false);
    stats.names.extend(labels);
    drop(stats);
    for (index, (_, body)) in prepared.emitted.functions.iter_mut().enumerate() {
        body.prepend_marker_call(callee, Repr::I64, base + index as u64);
    }
    prepared.stats = Some(callee);

    if !HOOKED.swap(true, Ordering::SeqCst) {
        unsafe extern "C" {
            fn atexit(callback: extern "C" fn()) -> i32;
        }
        // SAFETY: `report` is a plain `extern "C" fn()`; `atexit` is the C runtime's.
        unsafe { atexit(report) };
    }
}

#[cfg(windows)]
fn peak_rss_mb() -> Option<f64> {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_nonpaged: usize,
        quota_nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c: Counters = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<Counters>() as u32;
    // SAFETY: `c` is a correctly laid-out PROCESS_MEMORY_COUNTERS and `cb` is its size.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    (ok != 0).then(|| c.peak_working_set as f64 / (1024.0 * 1024.0))
}

#[cfg(not(windows))]
fn peak_rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

/// Prints the report once (called at the end of `Compiled::run`, and from `atexit` as a backstop
/// for programs that leave through `process.exit`).
pub(crate) fn report_once() {
    if enabled() && HOOKED.load(Ordering::SeqCst) {
        report();
    }
}

extern "C" fn report() {
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    let stats = STATS.lock().unwrap_or_else(|e| e.into_inner());
    let compiled = stats.names.len();
    let called = stats.called.iter().filter(|c| **c).count();
    let js = rts_codegen::emit::JS_FUNCTIONS_EMITTED.load(Ordering::Relaxed);
    let internal = stats.names.iter().filter(|n| n.starts_with("<internal")).count();
    let percent = if compiled == 0 { 0.0 } else { called as f64 * 100.0 / compiled as f64 };
    eprintln!(
        "[compile-stats] compiled={compiled} called={called} called/compiled={percent:.1}%  \
         (js_function_bodies_emitted={js}, internal/module-entry/wrapper={internal})"
    );
    // Split by unit: std (rts's own TypeScript prelude, compiled into the program) vs user files.
    let (mut std_c, mut std_k, mut user_c, mut user_k) = (0usize, 0usize, 0usize, 0usize);
    let mut per_unit: Vec<(usize, usize)> = vec![(0, 0); stats.units.len()];
    for (i, unit) in stats.unit.iter().enumerate() {
        let called = stats.called[i] as usize;
        match stats.units.get(*unit as usize) {
            Some((_, true)) => {
                std_c += 1;
                std_k += called;
            }
            _ => {
                user_c += 1;
                user_k += called;
            }
        }
        if let Some(row) = per_unit.get_mut(*unit as usize) {
            row.0 += 1;
            row.1 += called;
        }
    }
    let pct = |k: usize, c: usize| if c == 0 { 0.0 } else { k as f64 * 100.0 / c as f64 };
    eprintln!(
        "[compile-stats] user units: compiled={user_c} called={user_k} ({:.1}%) | std units: compiled={std_c} called={std_k} ({:.1}%)",
        pct(user_k, user_c),
        pct(std_k, std_c)
    );
    for ((specifier, is_std), (c, k)) in stats.units.iter().zip(&per_unit) {
        if !*is_std {
            eprintln!("[compile-stats]   user unit {specifier}: compiled={c} called={k} ({:.1}%)", pct(*k, *c));
        }
    }
    let wall = START.get().map(|s| s.elapsed().as_secs_f64() * 1e3).unwrap_or(0.0);
    let rss = peak_rss_mb().map(|m| format!("{m:.0} MB")).unwrap_or_else(|| "n/a".into());
    eprintln!(
        "[compile-stats] time: emit_modules={:.1} ms, prepare(+instrument)={:.1} ms, \
         place(cranelift)={:.1} ms, wall-since-compile-start-to-exit={wall:.1} ms, peak_rss={rss}",
        stats.emit.as_secs_f64() * 1e3,
        stats.prepare.as_secs_f64() * 1e3,
        stats.place.as_secs_f64() * 1e3,
    );
    if verbose() {
        let never: Vec<&str> = stats
            .names
            .iter()
            .zip(&stats.called)
            .filter(|(_, c)| !**c)
            .map(|(n, _)| n.as_str())
            .collect();
        let was: Vec<&str> = stats
            .names
            .iter()
            .zip(&stats.called)
            .filter(|(_, c)| **c)
            .map(|(n, _)| n.as_str())
            .collect();
        eprintln!("[compile-stats] CALLED ({}): {}", was.len(), was.join(", "));
        eprintln!(
            "[compile-stats] COMPILED-NOT-CALLED ({}), first 60: {}",
            never.len(),
            never.iter().take(60).copied().collect::<Vec<_>>().join(", ")
        );
    }
}
