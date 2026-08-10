//! `rts:runtime` — the module table, from inside a running program.
//!
//! # What this is for
//!
//! RTS has no ESM/CJS split: `import` and `require` reach one table, because
//! there is one. So a program can ask what it is made of and change it, and this
//! is that surface — `module` and `require` as two views of the same thing,
//! which is what makes `{ module, require }.resolve(…)` answer the same
//! specifier either way.
//!
//! # Reuse-check
//!
//! The table is `rts-core`'s `modules`, and nothing here holds a second one.
//! `module_at_name`, `module_specifiers` and `forget_module` were added to that
//! crate's host surface for this; a cache of my own beside it would be a second
//! answer to what a specifier resolves to, which is the failure the export
//! lowering was written to avoid on the other side.
//!
//! # What is real, and what is refused by name
//!
//! Real: `resolve`, `list`, `has`, `get`/`require`, `delete`.
//!
//! **`reload` is refused.** Re-running a module means compiling into the region
//! the program is already running in, and this engine compiles a program into a
//! region it creates — the same wall `node:vm` meets. `entry::evaluate` would
//! give a NEW region, and a module reloaded there would publish exports made of
//! cells the running program cannot touch. A `reload` that answered anything at
//! all would be handing back a namespace whose every value is unusable.
//!
//! **`watch` is refused.** Watching a file is `node:fs`'s, and this crate does
//! not depend on `rts-node` — the dependency goes the other way. A program
//! wanting it writes `fs.watch(module.resolve(x), …)`, which is the same thing
//! without a second watcher table.
//!
//! Both say so through `undefined` and through this list, rather than through a
//! silent no-op that looks like it worked.
//!
//! # `resolve`, and the divergence it carries
//!
//! Node resolves relative to the CALLING module. Nothing here knows which module
//! is calling — a native is handed a receiver and four values, not a stack of
//! module frames — so a relative specifier resolves against the process's
//! working directory instead. An absolute or already-resolved specifier is
//! answered unchanged, which is what a program passing `module.resolve` the
//! output of a previous `module.resolve` needs.

use rts_core::entry::{self, Context, Provided};

/// The members `module` and `require` both carry.
const MEMBERS: &[(&str, Provided)] = &[
    ("resolve", resolve),
    ("list", list),
    ("has", has),
    ("get", get),
    ("delete", forget),
    ("reload", refused),
    ("watch", refused),
];

/// The namespace `rts:runtime` is.
///
/// `module` and `require` are the SAME object under two names, not two objects
/// with the same members: a program comparing `module.resolve === require.resolve`
/// gets `true`, and there is one place a member can be replaced.
pub fn namespace(context: &mut Context) -> u64 {
    let namespace = entry::make_namespace(context, &[]);
    let table = entry::make_namespace(context, MEMBERS);
    entry::put_member(context, namespace, "module", table);
    entry::put_member(context, namespace, "require", table);
    namespace
}

/// `module.resolve(specifier)` — the name the table knows a module by.
extern "C" fn resolve(_e: u64, _this: u64, specifier: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::text_of(specifier) else {
        return entry::undefined_value();
    };
    let resolved = match text.starts_with("./") || text.starts_with("../") {
        false => text,
        true => {
            let base = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let joined = base.join(&text);
            // Canonicalised so two spellings of one file answer one specifier —
            // the same rule the loader applies, and the reason the answer can be
            // compared with what `list` reports.
            joined
                .canonicalize()
                .unwrap_or(joined)
                .display()
                .to_string()
        }
    };
    entry::with_runtime(|context| entry::make_string(context, &resolved))
}

/// `module.list()` — every specifier the table holds.
extern "C" fn list(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let names = entry::module_specifiers(context)
            .iter()
            .map(|name| entry::make_string(context, name))
            .collect();
        entry::make_array_in(context, names)
    })
}

/// `module.has(specifier)`.
extern "C" fn has(_e: u64, _this: u64, specifier: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::text_of(specifier) else {
        return entry::boolean_value(false);
    };
    let found = entry::with_runtime(|context| {
        entry::module_at_name(context, &text) != entry::undefined_in(context)
    });
    entry::boolean_value(found)
}

/// `require(specifier)` / `module.get(specifier)` — the namespace, or
/// `undefined`.
///
/// It cannot LOAD: compiling a file at run time would give it a region of its
/// own, and its exports would be cells this program cannot reach. So this
/// answers what the program was built with, which is every module of its own
/// graph plus every one the host provided.
extern "C" fn get(_e: u64, _this: u64, specifier: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::text_of(specifier) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| entry::module_at_name(context, &text))
}

/// `module.delete(specifier)` — forgets the name, answering whether one went.
///
/// What it does not do is unrun the module; see [`entry::forget_module`]'s own
/// doc. Anything already imported keeps its value, because an import read it
/// once.
extern "C" fn forget(_e: u64, _this: u64, specifier: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::text_of(specifier) else {
        return entry::boolean_value(false);
    };
    let removed = entry::with_runtime(|context| entry::forget_module(context, &text));
    entry::boolean_value(removed)
}

/// `reload` and `watch`: present, and honest about answering nothing.
///
/// Present rather than absent so that a program calling one fails at the call
/// with a value it can test, instead of at a property read on `undefined` — and
/// so the module doc has somewhere to name them.
extern "C" fn refused(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}
