//! `import.meta`, and `import()` — the two module operations that are not a
//! read of a specifier the compiler already resolved.
//!
//! # Why they are here and not in [`super::modules`]
//!
//! Not only because that file is past this crate's 500-line ceiling. The two
//! operations there — [`super::modules::module_binding`] and
//! [`super::modules::module_namespace`] — take a literal INDEX, because the host
//! resolved every static specifier before the program was compiled. Neither of
//! these can: `import.meta` is about the module doing the asking, and
//! `import(name)` computes its specifier while the program runs. What they share
//! with that file is the one table keyed by specifier, which they reach rather
//! than duplicate.
//!
//! # What this does NOT do, and who does
//!
//! Resolve a path. `"./x"` means different files in different directories, and
//! turning it into one is the host's — exactly as [`super::modules`] states for
//! a static import, and as `rts-host`'s loader does by rewriting the tree. A
//! dynamic specifier has nothing in the tree to rewrite, so the question moves
//! to run time and the capability comes DOWN: a host installs a [`Resolver`],
//! the same injection [`super::modules::Evaluator`] already is and for the same
//! reason — this crate is below the one that owns paths, so a call up would be a
//! dependency cycle.
//!
//! Nor does it LOAD a module. A specifier that resolves to something nothing
//! compiled is a rejected promise naming it, not a file read: reading a file is
//! the host's too, and a version that answered an empty namespace instead would
//! be the silent wrong answer this repository refuses everywhere else.

use super::modules::literal_text;
use super::objects::undefined_of;
use super::{Context, with_current};

/// How a host turns `(referrer, specifier)` into the name the module table is
/// keyed by.
///
/// A `fn` pointer and not a closure, for [`super::modules::Builder`]'s reason: a
/// closure would capture, and what a host would capture is the context.
pub type Resolver = fn(&str, &str) -> Option<String>;

/// Installs the host's specifier resolver.
pub fn declare_resolver(context: &mut Context, resolver: Resolver) {
    context.resolver = Some(resolver);
}

/// What `(referrer, specifier)` names, through the host's own resolver.
///
/// # Why a surface crate needs to reach this
///
/// Because the specifier table is keyed by what the LOADER produced, and any
/// other way of arriving at that key is a second resolver that must agree with
/// the first. `rts-node`'s `createRequire` reproduced the loader's rule —
/// join, canonicalise — and its own comment said it had to match "exactly".
/// It stopped matching the day the loader started stripping Windows's verbatim
/// `\\?\` prefix: `require` looked up a key nothing had registered and answered
/// `undefined`, which is the failure that rule was written to prevent and could
/// not prevent by being written down.
///
/// `None` where the host has no answer — a bare or `node:` specifier is not a
/// path, and the caller uses the text as written, which is the same rule the
/// loader applies.
pub fn resolve_specifier(context: &Context, from: &str, specifier: &str) -> Option<String> {
    context.resolver.and_then(|resolve| resolve(from, specifier))
}

/// Records the object `import.meta` answers for one module.
///
/// # Why the host builds the object rather than describing it
///
/// Because what is IN it is entirely the host's: the URL of a file it resolved,
/// and whether that file is the one the user named. A version taking a URL and a
/// flag would put the host's two facts behind this crate's idea of what
/// `import.meta` has, and the next field the host wants to add would need a
/// change here to carry it.
///
/// The module need not have been registered yet — a module that exports nothing
/// has no namespace, and its `import.meta` is still a real object.
pub fn declare_module_meta(context: &mut Context, specifier: &str, meta: u64) {
    if let Some(held) = context
        .modules
        .iter_mut()
        .find(|held| held.specifier == specifier)
    {
        held.meta = Some(meta);
        return;
    }
    context.modules.push(super::modules::Registered {
        specifier: specifier.to_owned(),
        namespace: None,
        build: None,
        provided: false,
        entry: None,
            running: false,
            meta: Some(meta),
        common: None,
        holder: None,
    });
}

/// `import.meta`, for the module whose specifier is at `referrer`.
///
/// # Why a miss throws rather than answering an object
///
/// An `import.meta` with no `url` on it is a surface that cannot do what its
/// name means, and a program reading `import.meta.url` off one gets `undefined`
/// somewhere far from the cause. A host that ran a module without describing it
/// is a wiring fault in this engine, so it says so at the point it is read.
#[rtse::entry]
pub fn import_meta(referrer: i64) -> u64 {
    let found = with_current(|context| {
        let text = literal_text(context, referrer)?;
        context
            .modules
            .iter()
            .find(|held| held.specifier == text)
            .and_then(|held| held.meta)
    });
    match found {
        Some(meta) => meta,
        None => {
            super::throw::plain_error(
                "import.meta is not available: nothing described this module to the runtime",
            );
            super::modules::undefined_value()
        }
    }
}

/// `import(specifier)` — a promise for the module's namespace.
///
/// # Why the namespace is the SAME object a static import would read
///
/// Because there is one table, and this is a read of it. `import("./m")` twice
/// answers one namespace, which is what makes `first === second` true — the
/// module cache is not a second mechanism here, it is the fact that a specifier
/// resolves to one entry.
///
/// # Why it is already resolved rather than settled later
///
/// The module has already RUN: the host compiles the whole graph, dependencies
/// first, before the entry starts. So there is nothing left to wait for, and a
/// promise that settled a turn later would be pretending to load something.
/// The divergence that leaves is stated rather than hidden — a module reached
/// only by `import()` is evaluated eagerly with the rest of the graph, where the
/// language evaluates it at the call.
/// Runs a module's body, if a module of that specifier still owes one.
///
/// # Why this exists, and why it is OUTSIDE every borrow
///
/// A module used to run in a topological sweep before the program started, and
/// a cycle has no topological order — so a cycle was refused by the loader
/// (#2852). Node runs a module when something NAMES it, which is the only shape
/// a cycle works in: the module being re-entered is already registered, with a
/// `module.exports` to hand back half filled.
///
/// Compiled code re-enters the context, so the body cannot run inside a
/// `with_current`: that is the re-entrant borrow this crate's layout exists to
/// make impossible, and it aborts rather than failing. So the question and the
/// answer are two passes, which is the same shape `module_binding` already uses
/// for raising — `modules::module_pending` says which body is owed and creates
/// the namespace before leaving the borrow, and this calls it with none held.
///
/// # The three states, and which one is the cycle
///
/// `module_pending` answers `None` for a module that has no body, has already
/// run, or is RUNNING. The last is the cycle: the second ask takes the namespace
/// as it stands instead of starting the body again, which is what makes
/// `require` of a module mid-flight answer a partial object rather than recurse
/// for ever.
///
/// # Rule 8
///
/// A module body is user code, so a throw out of it is checked here and left in
/// flight: the caller — `module_binding`, `module_namespace`, `module_import` —
/// returns without reading a namespace the body never finished filling, and the
/// compiled call site above re-raises. Reading it anyway would publish a half
/// built namespace as if the module had succeeded.
fn ensure_ran(specifier: &str) {
    let pending = with_current(|context| super::modules::module_pending(context, specifier));
    let Some(body) = pending else {
        return;
    };
    let nothing = with_current(|context| undefined_of(context));
    body(nothing, nothing, nothing, nothing, nothing, nothing);
    // Cleared whether the body finished or threw: a module that threw does not
    // get a second chance at running in this process, which is Node's rule too
    // (`require` of a module that threw once throws the SAME error again from a
    // cache). What a throw does change is what the caller does next, and
    // `super::throw::in_flight` is how it finds out.
    with_current(|context| super::modules::module_ran(context, specifier));
}

/// [`ensure_ran`], for the two entry points in `modules` that read a namespace.
///
/// `pub(in crate::entry)` rather than a second copy there: the run is one rule
/// and the borrow discipline around it is the delicate half.
pub(in crate::entry) fn ensure_module_ran(specifier: &str) {
    ensure_ran(specifier);
}

/// `import(specifier)` — the module namespace, as a promise.
///
/// The doc that stood here moved into [`ensure_ran`] along with the behaviour it
/// described: resolving a specifier is one question and RUNNING the module it
/// names is another, and this entry point now asks both.
///
/// A rejected promise and not a throw for every failure: `import()` is an
/// expression whose value is a promise, so a specifier nothing registered and a
/// module body that threw both travel as rejections rather than unwinding out of
/// the expression.
#[rtse::entry]
pub fn module_import(specifier: u64, referrer: i64) -> u64 {
    // Two passes, for the reason `module_binding` records: raising takes its
    // own borrow, because building the error runs the program's constructor.
    let found = with_current(|context| {
        let absent = undefined_of(context);
        let Some(wanted) = super::modules::string_in(context, specifier) else {
            return Err(String::from(
                "import() takes a string specifier, and was given something else",
            ));
        };
        let from = literal_text(context, referrer).unwrap_or_default();
        // The host's answer first, its own text second: a bare or `node:`
        // specifier is not a path and the resolver leaves it alone, which is
        // the same rule the loader applies to a static import.
        let resolved = context
            .resolver
            .and_then(|resolve| resolve(&from, &wanted))
            .unwrap_or_else(|| wanted.clone());
        let _ = absent;
        Ok((resolved, wanted))
    });
    // Resolved under the borrow, RUN outside it, read back under a second one.
    // Three passes rather than two, and the middle one is the whole point: a
    // module of this program has a body that has not run until something asks
    // for it, and `import()` asking is one of the three ways.
    let found = match found {
        Err(why) => Err(why),
        Ok((resolved, wanted)) => {
            ensure_ran(&resolved);
            if super::throw::in_flight() {
                // The body threw. `import()` is a promise, so the rejection
                // travels as one rather than as a throw out of an expression —
                // and the pending throw is taken because it becomes the
                // rejection reason instead of propagating from here.
                let reason = super::throw::caught().unwrap_or_else(|| with_current(|context| undefined_of(context)));
                return super::promise::rejected_with(reason);
            }
            with_current(|context| match context.module_at(&resolved) {
                Some(namespace) => Ok(namespace),
                None => Err(format!(
                    "cannot resolve module \"{wanted}\" — nothing registered that specifier"
                )),
            })
        }
    };
    match found {
        Ok(namespace) => super::promise::resolved_with(namespace),
        // A rejected promise and not a throw: `import()` is an expression that
        // answers a promise, and the language reports a failure to load through
        // it. A throw here would be uncatchable by the `catch` a program wrote.
        Err(message) => {
            let reason = super::throw::make_named_error("Error", &message)
                .unwrap_or_else(super::modules::undefined_value);
            super::promise::rejected_with(reason)
        }
    }
}
