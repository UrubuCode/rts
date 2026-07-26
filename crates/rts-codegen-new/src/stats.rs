//! Optimization instrumentation (FUTURE_OPTIMIZATION.md phase 0) — opt-in via
//! `RTS_REPR_STATS=1`.
//!
//! We already measure execution coverage (`scripts/measure_new.sh`) and wall time
//! (`crate::timing`). Neither answers the question that decides where optimization
//! work goes: **where does the representation proof fail, and what does the hot
//! loop actually pay per iteration?** Reading Cranelift IR by hand answers that for
//! ONE function; this answers it for a whole program, aggregated, with the ENGINE
//! source line that made each decision.
//!
//! Four event kinds, matching the four costs in `docs/specs/FUTURE_OPTIMIZATION.md`:
//!
//! | Event | What it costs | Phase that removes it |
//! |---|---|---|
//! | [`box_widen`] | native → `PolyValue` box at a Tagged boundary | 1 (close the widenings) |
//! | [`unbox`] | `PolyValue` → native decode + tag select | 1 |
//! | [`tagged_binding`] | a local/param with NO proven repr (every use pays) | 1 |
//! | [`runtime_call`] | an extern `call` — a full optimization barrier | 2/3/5 |
//!
//! ## How a site is identified
//!
//! Each record captures three coordinates, and the cross-product is what makes the
//! dump actionable:
//!
//! - **The engine site** — `#[track_caller]` gives the `file:line` in the LOWERING
//!   that made the decision, with zero edits at the ~165 call sites. That is the
//!   line to go fix.
//! - **The user function** + whether we were **inside a loop** (`loop_stack`
//!   depth > 0) — the same box costs 1 or 10^7 depending on this, so the dump ranks
//!   in-loop events first.
//! - **Prelude vs user code** — the embedded `.ts` prelude is ~5k lines and would
//!   otherwise drown the user program in the histogram, so the two are counted
//!   separately and the prelude is reported only as a summary line.
//!
//! ## Cost when disabled
//!
//! One `OnceLock<bool>` read per event and an early return. [`note_site`] (called
//! per statement) is the same. Nothing is allocated and no behavior changes —
//! this module can only observe.

use std::collections::HashMap;
use std::panic::Location;
use std::sync::{Mutex, OnceLock};

use crate::repr::Repr;

/// Is `RTS_REPR_STATS` set to something other than `0`/empty?
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("RTS_REPR_STATS") {
        Ok(v) => !v.is_empty() && v != "0",
        Err(_) => false,
    })
}

/// What kind of cost an event records.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Kind {
    /// A native value boxed into a `PolyValue` (widening to `Tagged`).
    Box,
    /// A `PolyValue` decoded back to a native repr.
    Unbox,
    /// A local or param bound with `Repr::Tagged` — no proven representation.
    TaggedBinding,
    /// An extern runtime `call` emitted (optimization barrier; may allocate).
    Call,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Box => "BOX",
            Kind::Unbox => "UNBOX",
            Kind::TaggedBinding => "TAGGED-BINDING",
            Kind::Call => "RUNTIME-CALL",
        }
    }
}

/// One aggregation bucket. Two events merge iff every coordinate matches.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    kind: Kind,
    /// Engine source position that made the decision (`#[track_caller]`).
    site: (&'static str, u32),
    /// The user function being lowered (`__rtsn_main`, `__rtsn_method_C_m`, …).
    user_fn: String,
    /// Whether the lowering was inside at least one enclosing loop.
    in_loop: bool,
    /// Repr name / runtime symbol / binding name, depending on `kind`.
    detail: String,
}

/// The per-lowering context the record functions read. Updated by the `Lowerer`
/// as it walks statements; thread-local because `parcompile` machine-compiles in
/// parallel (each worker lowers its own functions).
#[derive(Clone)]
struct Site {
    user_fn: String,
    in_loop: bool,
    is_prelude: bool,
}

impl Default for Site {
    fn default() -> Self {
        Site {
            user_fn: String::from("<unknown>"),
            in_loop: false,
            is_prelude: false,
        }
    }
}

thread_local! {
    static SITE: std::cell::RefCell<Site> = std::cell::RefCell::new(Site::default());
}

/// User-code buckets → count. Prelude events are folded into [`PRELUDE`] instead,
/// so the ~5k-line embedded prelude cannot drown the user program's histogram.
fn agg() -> &'static Mutex<HashMap<Key, u64>> {
    static AGG: OnceLock<Mutex<HashMap<Key, u64>>> = OnceLock::new();
    AGG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Prelude-origin event counts, by kind only (summary line, never a histogram).
fn prelude_agg() -> &'static Mutex<HashMap<Kind, u64>> {
    static P: OnceLock<Mutex<HashMap<Kind, u64>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---- recording API ----------------------------------------------------------

/// Record the lowering position: which user function, whether inside a loop, and
/// whether this is prelude code. Called once per statement by the `Lowerer`
/// (statement granularity is exact for loop depth — `loop_stack` only changes at
/// statement boundaries).
pub fn note_site(user_fn: &str, loop_depth: usize, is_prelude: bool) {
    if !enabled() {
        return;
    }
    SITE.with(|s| {
        let mut s = s.borrow_mut();
        if s.user_fn != user_fn {
            s.user_fn.clear();
            s.user_fn.push_str(user_fn);
        }
        s.in_loop = loop_depth > 0;
        s.is_prelude = is_prelude;
    });
}

fn record(kind: Kind, detail: String, loc: &'static Location<'static>) {
    if !enabled() {
        return;
    }
    let site = SITE.with(|s| s.borrow().clone());
    if site.is_prelude {
        if let Ok(mut p) = prelude_agg().lock() {
            *p.entry(kind).or_insert(0) += 1;
        }
        return;
    }
    let key = Key {
        kind,
        site: (loc.file(), loc.line()),
        user_fn: site.user_fn,
        in_loop: site.in_loop,
        detail,
    };
    if let Ok(mut a) = agg().lock() {
        *a.entry(key).or_insert(0) += 1;
    }
}

/// A native value was boxed into a `PolyValue` (widened to `Tagged`).
#[track_caller]
pub fn box_widen(from: Repr) {
    record(Kind::Box, format!("{from:?}"), Location::caller());
}

/// A `PolyValue` was decoded back to a native representation.
#[track_caller]
pub fn unbox(to: Repr) {
    record(Kind::Unbox, format!("{to:?}"), Location::caller());
}

/// A local or param was bound with `Repr::Tagged` — no proven representation, so
/// every read and write of it pays a tag check.
#[track_caller]
pub fn tagged_binding(name: &str) {
    record(Kind::TaggedBinding, name.to_string(), Location::caller());
}

/// An extern runtime `call` was emitted. Every one is a full optimization barrier
/// for the egraph (no inlining, no CSE across it, live values spill).
#[track_caller]
pub fn runtime_call(symbol: &str) {
    record(Kind::Call, symbol.to_string(), Location::caller());
}

// ---- reporting --------------------------------------------------------------

/// Heuristic: does this runtime symbol ALLOCATE? Name-based on purpose — the ABI
/// carries no allocation flag, and adding one is not worth it for a measurement
/// pass. Over-inclusive by design: a false positive shows up as a symbol the
/// reader can dismiss, a false negative hides a real allocation site.
fn allocates(sym: &str) -> bool {
    const MARKERS: [&str; 9] = [
        "_NEW", "_new", "ALLOC", "alloc", "REIFY", "reify", "STRING_FROM", "CONCAT", "_CLONE",
    ];
    MARKERS.iter().any(|m| sym.contains(m))
}

/// Print the histogram to stderr and clear the aggregate. No-op when disabled.
///
/// Called at the end of module population, so it covers both the JIT (`rts run`/
/// `test`/`eval`) and AOT (`rts compile`) paths, which share that function.
pub fn dump() {
    if !enabled() {
        return;
    }
    let entries: Vec<(Key, u64)> = match agg().lock() {
        Ok(mut a) => a.drain().collect(),
        Err(_) => return,
    };
    let prelude: HashMap<Kind, u64> = match prelude_agg().lock() {
        Ok(mut p) => p.drain().collect(),
        Err(_) => HashMap::new(),
    };
    if entries.is_empty() && prelude.is_empty() {
        return;
    }

    let total = |k: Kind, loop_only: bool| -> u64 {
        entries
            .iter()
            .filter(|(key, _)| key.kind == k && (!loop_only || key.in_loop))
            .map(|(_, n)| *n)
            .sum()
    };

    eprintln!();
    eprintln!("=== rts repr/alloc stats (RTS_REPR_STATS) ===================");
    eprintln!("user code — emitted lowering events:");
    eprintln!(
        "  {:<16} {:>10} {:>12}",
        "kind", "total", "in-loop"
    );
    for k in [Kind::Box, Kind::Unbox, Kind::TaggedBinding, Kind::Call] {
        eprintln!(
            "  {:<16} {:>10} {:>12}",
            k.label(),
            total(k, false),
            total(k, true)
        );
    }
    let prelude_total: u64 = prelude.values().sum();
    if prelude_total > 0 {
        eprintln!(
            "  (prelude, excluded from the histograms below: {prelude_total} events)"
        );
    }

    // The in-loop events are the ones that multiply by trip count — rank them
    // first and separately, because a box outside a loop is noise next to one
    // inside it.
    section(&entries, Kind::Box, true, "BOX widenings INSIDE loops");
    section(&entries, Kind::Unbox, true, "UNBOX decodes INSIDE loops");
    section(
        &entries,
        Kind::Call,
        true,
        "RUNTIME CALLS INSIDE loops (each = an optimization barrier)",
    );
    section(&entries, Kind::TaggedBinding, false, "TAGGED bindings (no proven repr)");
    section(&entries, Kind::Box, false, "BOX widenings (all sites)");
    alloc_section(&entries);
    eprintln!("============================================================");
    eprintln!();
}

/// Print the top buckets of one kind, sorted by count.
fn section(entries: &[(Key, u64)], kind: Kind, loop_only: bool, title: &str) {
    const TOP: usize = 20;
    let mut rows: Vec<&(Key, u64)> = entries
        .iter()
        .filter(|(k, _)| k.kind == kind && (!loop_only || k.in_loop))
        .collect();
    if rows.is_empty() {
        return;
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.detail.cmp(&b.0.detail)));
    eprintln!();
    eprintln!("-- {title} --");
    eprintln!(
        "  {:>7}  {:<28} {:<26} {}",
        "count", "detail", "user fn", "engine site"
    );
    for (key, n) in rows.iter().take(TOP) {
        eprintln!(
            "  {:>7}  {:<28} {:<26} {}:{}",
            n,
            trunc(&key.detail, 28),
            trunc(&key.user_fn, 26),
            short_path(key.site.0),
            key.site.1
        );
    }
    if rows.len() > TOP {
        let rest: u64 = rows.iter().skip(TOP).map(|(_, n)| *n).sum();
        eprintln!("  {:>7}  … {} more buckets", rest, rows.len() - TOP);
    }
}

/// Allocation sites, derived from the runtime-call buckets by symbol name.
fn alloc_section(entries: &[(Key, u64)]) {
    const TOP: usize = 15;
    let mut by_sym: HashMap<(&str, bool), u64> = HashMap::new();
    for (key, n) in entries {
        if key.kind == Kind::Call && allocates(&key.detail) {
            *by_sym
                .entry((key.detail.as_str(), key.in_loop))
                .or_insert(0) += n;
        }
    }
    if by_sym.is_empty() {
        return;
    }
    let mut rows: Vec<((&str, bool), u64)> = by_sym.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.0.cmp(b.0.0)));
    eprintln!();
    eprintln!("-- ALLOCATION sites (heuristic, by symbol name) --");
    eprintln!("  {:>7}  {:<8} {}", "count", "in-loop", "symbol");
    for ((sym, in_loop), n) in rows.iter().take(TOP) {
        eprintln!(
            "  {:>7}  {:<8} {}",
            n,
            if *in_loop { "yes" } else { "no" },
            sym
        );
    }
}

/// Strip the crate prefix from an engine path so the site column stays readable.
fn short_path(p: &str) -> &str {
    match p.rfind("src/") {
        Some(i) => &p[i + 4..],
        None => match p.rfind("src\\") {
            Some(i) => &p[i + 4..],
            None => p,
        },
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let keep: String = s.chars().take(n.saturating_sub(1)).collect();
    format!("{keep}…")
}
