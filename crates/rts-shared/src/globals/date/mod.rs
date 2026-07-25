//! `Date` global class.
//!
//! Migrated (DRAIN_MOTOR) from the hand-written `Member` table + per-method
//! `#[no_mangle] extern "C"` fns over the bespoke `Entry::DateMs(i64)` storage to
//! the `#[rtse::class]` authoring macro (`instance.rs`): the instance is now a
//! normal Rust struct (`Date { ms }`) stored via the generic `Entry::Rtse` — the
//! same storage every macro-authored class uses; the dedicated `Entry::DateMs`
//! variant is deleted. The real calendar math still runs through the `date`
//! namespace backend (`crate::date::__RTS_FN_NS_DATE_*`).
//!
//! Ctors/getters/statics/string-methods/single-setters are macro-authored
//! (`instance.rs`). Left hand-written and APPENDED here via the class-reopen
//! pattern: the multi-component setters + their `setUTC*` aliases + `toGMTString`
//! (`setters.rs` — the `i64::MIN` keep-current sentinel is the opposite of the
//! macro's `optional=N` undefined/invalidate). The ToPrimitive coercion trio
//! (`coercion.rs`) and the `instanceof` predicate (`__RTS_FN_NS_GC_IS_DATE`, in
//! `rts-std`, now keyed on `rtse_class_of == "Date"`) round it out.

pub mod coercion;
pub mod fmt;
pub mod instance;
pub mod setters;

use rts_engine::{Engine, Member};

/// Register the global class `Date`. The `#[rtse::class]` in `instance.rs`
/// (`instance::register`) provides the ctors/getters/statics/string-methods/
/// single-setters; this wrapper re-opens the class to attach the class doc + the
/// `instanceof` predicate and to append the hand-written residuals
/// (`setters::append_members`). `ClassBuilder::done()` REPLACES (does not merge)
/// the Registry entry, so the macro members are read back and replayed in the
/// single `.done()`.
pub fn register_class_spec(e: &mut Engine) {
    // 1) Macro-authored ctors + getters + statics + string methods + single setters.
    instance::register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("Date")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    // 2) Re-open: set the class doc + instanceof predicate, replay the macro
    //    members, then append the hand-written multi-component setters + aliases.
    let mut cb = e
        .class("Date")
        .doc("Built-in Date class. Stores UTC timestamp as ms since Unix epoch.")
        .instanceof_predicate("__RTS_FN_NS_GC_IS_DATE");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    setters::append_members(cb).done();
}
