//! `node:trace_events` — the category bookkeeping is real; nothing is ever
//! recorded.
//!
//! # Reuse-check
//!
//! `rts-cranelift` has no tracing/instrumentation surface at all (checked
//! `src/probe/` — that is this repository's OWN perf-measurement harness,
//! `docs/reference/node/trace_events.md` §5.1 already names it as
//! unrelated and un-shareable: a probe for measuring RTS itself, not a sink
//! for a program's Chrome-Trace-Event-Format output). `rts_core::entry`
//! has no category registry either. The nearest existing shape is
//! [`crate::diagnostics_channel`]'s `CHANNELS: Mutex<HashMap<String, u64>>`
//! — a process-wide registry keyed by a name a program supplies — and this
//! module's category-reference-count table follows the same shape for the
//! same reason: a category, like a channel, is identified by the STRING a
//! program passes in, not by any identity this crate mints.
//!
//! # What is real, and what is not
//!
//! `createTracing`, `tracing.enable()`/`.disable()`, and
//! `getEnabledCategories()` are a genuine reference-counted set: enabling the
//! same category from two `Tracing` objects and disabling it from one leaves
//! it enabled, exactly per `docs/reference/node/trace_events.md` §4. What
//! does NOT exist is a tracing sink: no file writer, no
//! `TraceEventRecord`, no producer call site anywhere in this engine emits
//! one. `getEnabledCategories()` answers a truthful set of NAMES; nothing
//! reads that set to decide whether to instrument anything, because nothing
//! instruments anything. A program that enables `"v8"` and expects a
//! `node_trace.*.log` to appear gets bookkeeping and no file — that is the
//! deliberate, complete scope of this module, not a partial version of a
//! larger one in progress.
//!
//! # Not implemented, by name
//!
//! - **The trace-event core: a `TraceEventRecord`, a file writer, and every
//!   producer** (GC, compile, fs, net, console, promise-rejection — the ~20
//!   categories the spec catalogs). Per the spec's own §5.1, this belongs in
//!   `rts-engine` (the one crate below every consumer, since `v8`-category
//!   events originate inside the GC/compiler, which cannot depend upward on
//!   `rts-node`) — nothing this crate owns.
//! - **The CLI-supplied category floor** (`--trace-event-categories` and
//!   equivalents). No RTS CLI flag populates one; the registry starts, and
//!   stays, exactly as empty as whatever a program's own `createTracing`
//!   calls make it.
//! - **Worker-thread unavailability.** No `node:worker_threads` exists in
//!   this engine yet for the "not available inside a Worker" rule to apply
//!   to.

use rts_core::entry::{self, Provided};
use std::collections::HashMap;
use std::sync::Mutex;

static CATEGORIES: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);

fn with_categories<T>(body: impl FnOnce(&mut HashMap<String, u32>) -> T) -> T {
    let mut guard = CATEGORIES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn split(text: &str) -> Vec<String> {
    text.split(',').map(str::trim).filter(|part| !part.is_empty()).map(str::to_owned).collect()
}

fn enabled_categories() -> String {
    with_categories(|table| {
        let mut names: Vec<&String> = table.iter().filter(|&(_, &count)| count > 0).map(|(name, _)| name).collect();
        names.sort();
        names.into_iter().cloned().collect::<Vec<_>>().join(",")
    })
}

const TRACING_METHODS: &[(&str, Provided)] = &[("enable", enable), ("disable", disable)];

/// The namespace `node:trace_events` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    let members: &[(&str, Provided)] = &[("createTracing", create_tracing), ("getEnabledCategories", get_enabled_categories)];
    entry::make_namespace(context, members)
}

/// The `categories: string[]` field of `createTracing`'s options object,
/// joined into the same comma-separated form the rest of this module works
/// in — coercing each element the way the spec's §2.2 requires (numbers and
/// other primitives coerce; an object with no meaningful text form is
/// dropped rather than causing this native to throw, which it cannot do —
/// see [`crate::assert`]'s module doc for the same wall).
/// Reads `options.categories[]`, one borrow to find the array
/// ([`entry::get_member`] is context-taking-only) followed by an unheld walk
/// of it ([`entry::get_indexed`]/[`entry::text_of`] are ambient-only and open
/// their own borrow per call) — never both nested, since a native here holds
/// at most one borrow at a time.
fn categories_of(options: u64) -> Vec<String> {
    let list = entry::with_runtime(|context| entry::get_member(context, options, "categories"));
    let undefined = entry::undefined_value();
    let mut out = Vec::new();
    let mut index = 0usize;
    loop {
        let key = entry::make_number(index as f64);
        let element = entry::get_indexed(list, key);
        if element == undefined {
            break;
        }
        if let Some(text) = entry::text_of(element) {
            out.push(text);
        }
        index += 1;
        if index > 4096 {
            break;
        }
    }
    out
}

/// `trace_events.createTracing({ categories })`.
extern "C" fn create_tracing(_e: u64, _namespace: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let categories = categories_of(options);
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Tracing", TRACING_METHODS);
        let instance = entry::make_instance(context, prototype);
        let joined = entry::make_string(context, &categories.join(","));
        entry::put_member(context, instance, "categories", joined);
        let enabled = entry::boolean_value(false);
        entry::put_member(context, instance, "enabled", enabled);
        instance
    })
}

/// `tracing.enable()` — increments every one of this `Tracing`'s categories,
/// once, guarded by its own `enabled` flag so a second call does not
/// double-count (see the reference doc's §4 for why an unguarded call would
/// leak a category enabled forever after one `disable()`).
extern "C" fn enable(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let already = entry::get_member(context, this, "enabled");
        if already == entry::boolean_value(true) {
            return entry::undefined_in(context);
        }
        let categories = entry::get_member(context, this, "categories");
        let Some(text) = entry::text_in(context, categories) else {
            return entry::undefined_in(context);
        };
        with_categories(|table| {
            for name in split(&text) {
                *table.entry(name).or_insert(0) += 1;
            }
        });
        let enabled = entry::boolean_value(true);
        entry::put_member(context, this, "enabled", enabled);
        entry::undefined_in(context)
    })
}

/// `tracing.disable()` — the inverse of [`enable`], only on the
/// `enabled: true -> false` transition.
extern "C" fn disable(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let already = entry::get_member(context, this, "enabled");
        if already != entry::boolean_value(true) {
            return entry::undefined_in(context);
        }
        let categories = entry::get_member(context, this, "categories");
        let Some(text) = entry::text_in(context, categories) else {
            return entry::undefined_in(context);
        };
        with_categories(|table| {
            for name in split(&text) {
                if let Some(count) = table.get_mut(&name) {
                    *count = count.saturating_sub(1);
                }
            }
        });
        let enabled = entry::boolean_value(false);
        entry::put_member(context, this, "enabled", enabled);
        entry::undefined_in(context)
    })
}

/// `trace_events.getEnabledCategories()`.
extern "C" fn get_enabled_categories(_e: u64, _namespace: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, &enabled_categories()))
}
