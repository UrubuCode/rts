//! CommonJS, on the same table an `import` reads.
//!
//! # Why this is not a second module system
//!
//! `modules.rs` holds one table from a specifier to what it resolves to, and
//! says why there is only one: two answers to "what does this specifier mean"
//! disagree the first time a program reaches one module through both. CommonJS
//! is the second way of reaching it, not a second table — so what is here is a
//! second READ ([`require_function`]) and a second WRITE
//! ([`module_publish_common`]) of the entry `modules.rs` already keeps.
//!
//! The reuse check that preceded this found three things already answered, and
//! all three are called rather than rewritten: `context.resolver` — the host
//! hook `import()` resolves through — turns `"./x"` into the table's key from
//! the module that wrote it; `module_at` builds a lazily-registered namespace;
//! and `closure_new` mints a callable over an environment, which is what lets
//! two modules have two `require`s rooted at two files. Nothing here parses a
//! path, which is rule 1 of this crate: where a file is, is the host's.
//!
//! # What `require` answers, and why it is not always the namespace
//!
//! A CommonJS module's exports are `module.exports` — one value, which the body
//! may REPLACE (`module.exports = function () {}`), not a set of names. A
//! namespace object cannot represent that: a module exporting a function would
//! come back as an object with the function somewhere inside it.
//!
//! So the entry carries both. `common` is what the module last left in
//! `module.exports`, and `require` answers it when there is one. A module that
//! never mentions `module` or `exports` has none, and `require` of it answers
//! the namespace its `export` statements published — which is what makes
//! `require("./esm-module")` work rather than answer an empty object.
//!
//! # And why the two are published into each other
//!
//! [`module_publish_common`] also writes the value into the namespace, under
//! `default` and under each of its own keys, so `import x from "./cjs"` and
//! `import { a } from "./cjs"` both see what the CommonJS body produced. Node
//! does this by STATICALLY lexing the file for `exports.a = …`; this does it
//! from the finished object, which is the same intent measured rather than
//! guessed — a name the body computed is found here and missed there.

use super::{Context, with_current};

/// Where the requiring module's own specifier lives in the closure's
/// environment.
///
/// An array read back with the ambient `get_indexed`, which is the shape
/// `rts-node`'s `createRequire` already uses for the same job — the environment
/// needs no interned key that way.
const REFERRER: f64 = 0.0;

/// The `require` a module is given, rooted at the module that asked.
///
/// # Why a closure and not one global function
///
/// Because `"./x"` means different files in two directories, and a `require`
/// that could not tell which module called it would have to guess. The referrer
/// is captured at module entry — the emitter knows it, since it is the same
/// literal `import.meta` and a static `import` already cross with.
///
/// Rooted at a FILE rather than a directory: the host resolver takes
/// `(referrer, specifier)` exactly as it does for `import()`, so both forms
/// resolve through one function and cannot come to disagree.
#[rtse::entry]
pub fn require_function(own: i64) -> u64 {
    let environment = with_current(|context| {
        let text = super::modules::literal_text(context, own).unwrap_or_default();
        let held = super::modules::make_string(context, &text);
        super::modules::make_array_in(context, vec![held])
    });
    super::functions::closure_new(require_call as *const () as usize as i64, environment)
}

/// `require(id)`.
///
/// Answers the CommonJS value when the module left one, and its namespace
/// otherwise. A specifier nothing registered raises, rather than answering
/// `undefined`: a program that requires a module that is not there reads a
/// property of `undefined` two lines later, and the error names the wrong file.
extern "C" fn require_call(environment: u64, _this: u64, id: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Read BEFORE the borrow below: `get_indexed` is ambient and takes a borrow
    // of its own, so reading the environment inside `with_current` is the
    // re-entrant borrow this crate's entry points are careful never to make.
    let from = referrer_of(environment);
    // And the lookup answers before anything is raised, for the reason
    // `module_import` records: raising takes its own borrow, because building
    // the error runs the program's own constructor.
    let found = with_current(|context| {
        let Some(wanted) = super::modules::string_in(context, id) else {
            return Err(String::from(
                "require() takes a string specifier, and was given something else",
            ));
        };
        // The host's answer first and the text second, which is the rule the
        // loader applies to a static import: `node:fs` and a bare name are not
        // paths, and the resolver leaves them alone.
        let resolved = context
            .resolver
            .and_then(|resolve| resolve(&from, &wanted))
            .unwrap_or_else(|| wanted.clone());
        match value_of(context, &resolved) {
            Some(value) => Ok(value),
            // A bare specifier the host provides under `node:` — `require("fs")`
            // is what the whole Node corpus writes, and the table is keyed by
            // what the host registered. Tried second so a real file called `fs`
            // still wins.
            None => match value_of(context, &format!("node:{resolved}")) {
                Some(value) => Ok(value),
                None => Err(format!(
                    "cannot find module \"{wanted}\" — nothing registered that specifier"
                )),
            },
        }
    });
    match found {
        Ok(value) => value,
        Err(message) => {
            super::throw::plain_error(&message);
            super::modules::undefined_value()
        }
    }
}

/// What one specifier answers a `require`: the CommonJS value, or the namespace.
fn value_of(context: &mut Context, specifier: &str) -> Option<u64> {
    if let Some(held) = context
        .modules
        .iter()
        .find(|held| held.specifier == specifier)
        && let Some(common) = held.common
    {
        return Some(common);
    }
    // `module_at` and not the field, because a host module is registered lazily
    // and its namespace is built on the first read — the same call a static
    // import makes.
    context.module_at(specifier)
}

/// The specifier of the module a `require` closure was minted for.
fn referrer_of(environment: u64) -> String {
    let zero = super::modules::make_number(REFERRER);
    let held = super::computed::get_indexed(environment, zero);
    with_current(|context| super::modules::string_in(context, held)).unwrap_or_default()
}

/// Records what a module left in `module.exports`, and mirrors it into the
/// namespace an `import` reads.
///
/// Emitted after the body, for the reason `module::emit_publications` gives:
/// the value published is the one the module finished with, and a module that
/// assigns `module.exports` on its last line is exactly what that is for.
#[rtse::entry]
pub fn module_publish_common(own: i64, value: u64) -> u64 {
    with_current(|context| {
        let Some(specifier) = super::modules::literal_text(context, own) else {
            return super::modules::undefined_in(context);
        };
        let namespace = super::modules::namespace_for(context, specifier.clone());
        if let Some(held) = context
            .modules
            .iter_mut()
            .find(|held| held.specifier == specifier)
        {
            held.common = Some(value);
        }
        // `default` first: `import x from "./cjs"` is what Node's own interop
        // binds to the whole `module.exports`, and it is the form that works
        // for a module exporting a function.
        super::modules::put_member(context, namespace, "default", value);
        // Then each own name, so `import { a }` sees what `exports.a = …` set.
        // Read from the finished object rather than from the source text, which
        // is the difference from Node's static lexer stated in this module's
        // header.
        if super::modules::is_object(context, value) {
            for name in super::modules::member_names(context, value) {
                if name == "default" {
                    continue;
                }
                let member = super::modules::get_member(context, value, &name);
                super::modules::put_member(context, namespace, &name, member);
            }
        }
        super::modules::undefined_in(context)
    })
}
