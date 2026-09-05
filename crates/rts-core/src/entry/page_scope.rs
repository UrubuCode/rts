//! Reading and writing a page `<script>`'s free identifiers dynamically —
//! against the environment VALUE the page handed the script (its `window`),
//! never the process-wide global object [`super::global`] answers for.
//!
//! # Why a page script needs its own pair
//!
//! A UMD bundle's wrapper — `(function(global, factory) { … ;
//! factory(global.React = {}); }(this, function () {…})))` — writes `React`
//! as a property of `this`, which at the top of a page `<script>` IS the
//! page's own `window` (the explicit receiver
//! `rts_core::entry::evaluate_in_scope_with_receiver` passes). That write is
//! an ORDINARY property assignment on a value — nothing here is needed for
//! it. What needed a new pair is the SIBLING script that later reads `React`
//! as a bare identifier: nothing in ITS OWN text ever assigns it, so the
//! compiler cannot place it in the enclosing chain
//! `rts-codegen::emit::page`/`emit_page_program` build ahead of time (AOT) or
//! [`super::eval_scope::environment_names`] measures after script 0 ran
//! (JIT). Answering `UnboundName` at compile time, or `ReferenceError`
//! against [`super::global::holder`] (the PROCESS global, which a page
//! script never writes to), are both wrong — a real browser asks the SAME
//! window `global.React` was written to.
//!
//! # Why this is not `global_get_unbound`/`global_set` handed a different object
//!
//! Because neither has a parameter for one — both always reach for
//! [`super::global::holder`], which is deliberately the ONE process-wide
//! object every ordinary program shares. Widening them to take an object
//! would turn every existing call site into one more argument to thread
//! through for a feature only page scripts need — this is a second, small
//! pair instead, over the same `read_property`/`put`/`reference_error` this
//! crate already has.
//!
//! # Where the object comes from at the call site
//!
//! `rts-codegen`'s `emit::page` registers one reserved name at hop 0 of the
//! chain it already builds for `window_base`/`published`, purely so
//! `Scope`'s existing nested-function hop bookkeeping — which already
//! adjusts a captured name's hop count once per enclosing closure, for names
//! that ARE in the chain — carries a page's window to whatever depth a FREE
//! identifier is read or written at. Nothing about hop counting changes;
//! this module only answers what the value at that hop holds.

use super::objects::{put, read_property, undefined_of};
use super::with_current;
use crate::object::Key;
use crate::value::Value;

/// `key` off `environment`, or the `ReferenceError` a real browser raises for
/// the same free identifier read against the same `window`.
///
/// # Why it always tries the object before raising
///
/// Unlike [`super::global::global_get_unbound`], which is reached only after
/// the compiler proved a name unresolvable ANYWHERE and therefore always
/// raises, this is reached for every page-script identifier the STATIC chain
/// did not carry — some of which a SIBLING script may have set on `window`
/// since compile time (AOT) or since this program started (JIT, where the
/// chain measured at compile time can be stale the instant a later `<script>`
/// writes something new). So the object is asked first, and only a genuine
/// miss — nothing in the compiled program and nothing any script has run so
/// far put this name on the window — is the `ReferenceError` the language
/// gives the same read in a real browser.
#[rtse::entry]
pub fn page_global_get(environment: u64, key: i64) -> u64 {
    if let Some(found) = with_current(|context| {
        let name = u32::try_from(key).ok().and_then(|number| context.keys.key(number))?;
        let object = Value(environment).as_slot()?;
        read_property(context, object, Key::Name(name)).map(|value| value.bits())
    }) {
        return found;
    }
    // Collected and the borrow dropped before raising — a second `with_current`
    // nested inside the one above would be the re-entrant borrow rule 8 of
    // this crate's README exists to keep out of an `extern "C"` frame.
    let text = with_current(|context| {
        let name = u32::try_from(key).ok().and_then(|number| context.keys.key(number));
        name.and_then(|name| context.interner.text(name))
            .and_then(|text| text.to_rust())
    });
    let message = match text {
        Some(text) => format!("{text} is not defined"),
        None => "is not defined".to_owned(),
    };
    super::throw::reference_error(&message);
    // Never observed: `reference_error` leaves a pending throw, and the call
    // site this crosses (`rts-codegen::emit::expr::call`) checks for one
    // immediately and re-raises before the value is used — the same
    // convention `global_get_unbound`'s own tail states.
    with_current(|context| undefined_of(&*context))
}

/// Writes `key` onto `environment`, creating it if absent — sloppy mode's
/// global creation, aimed at a page's own window instead of the process
/// object [`super::global::global_set`] answers for.
///
/// Unconditional, same as `global_set`: the compiler only emits this call for
/// an assignment sloppy mode already decided creates a binding, so there is
/// no second question to ask here about whether the write is allowed.
#[rtse::entry]
pub fn page_global_set(environment: u64, key: i64, value: u64) -> u64 {
    with_current(|context| {
        let Ok(number) = u32::try_from(key) else {
            return value;
        };
        let Some(name) = context.keys.key(number) else {
            return value;
        };
        if let Some(object) = Value(environment).as_slot() {
            put(context, object, Key::Name(name), value);
        }
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Context, object_new, with_context};
    use crate::value::Singletons;

    /// A context installed for the duration, with keys already issued — the
    /// same fixture `global.rs`'s own tests use, for the same reason: an
    /// entry point needs an active `Context` to reach `with_current` at all.
    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let singletons = Singletons { undefined: 0, null: 1, hole: 2 };
        let context = Context::new(singletons, crate::value::Kinds::in_declaration_order());
        with_context(context, body).1
    }

    /// The number a name has, minted the way a host mints it — the same
    /// helper `global.rs`'s own tests use.
    fn key_of(name: &str) -> i64 {
        with_current(|context| {
            let text = crate::text::Str::from_str(name);
            context.interner.intern(&text, &mut context.keys).index() as i64
        })
    }

    /// The behaviour this whole module exists for: a name a SIBLING wrote as
    /// an ordinary property, read back by a script whose OWN compiled chain
    /// never carried it — the UMD `React` shape this module's header names.
    #[test]
    fn a_name_written_on_the_environment_by_another_script_reads_back() {
        hosted(|| {
            let environment = object_new(0);
            let value = object_new(0);
            let key = key_of("React");
            page_global_set(environment, key, value);
            let answer = page_global_get(environment, key);
            assert_eq!(
                answer, value,
                "a property another script's compiled write put on the SAME \
                 environment value must read back, exactly as a browser's \
                 global object would answer for `React` after a UMD bundle \
                 assigned it"
            );
        });
    }

    /// The other half of the claim: a name NOTHING has written yet raises —
    /// not `undefined`, and not a silent miss — matching what a real browser
    /// throws for the same free read.
    #[test]
    fn an_absent_name_raises_a_reference_error_rather_than_answering_undefined() {
        hosted(|| {
            let environment = object_new(0);
            let key = key_of("NeverWritten");
            let _ = page_global_get(environment, key);
            assert!(
                crate::entry::pending().is_some(),
                "the language's own answer for a free identifier no script \
                 ever set is ReferenceError, not undefined"
            );
        });
    }

    /// Two DIFFERENT environments never share a property — the whole reason
    /// this is parameterized rather than a second `holder(context)`.
    #[test]
    fn two_environments_do_not_see_each_others_writes() {
        hosted(|| {
            let window_a = object_new(0);
            let window_b = object_new(0);
            let value = object_new(0);
            let key = key_of("React");
            page_global_set(window_a, key, value);
            let _ = page_global_get(window_b, key);
            assert!(
                crate::entry::pending().is_some(),
                "window_b never had React written to it, so reading it there \
                 must still raise even though window_a's copy exists"
            );
        });
    }
}
