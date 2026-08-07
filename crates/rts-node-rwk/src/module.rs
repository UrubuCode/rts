//! `node:module` — `builtinModules`/`isBuiltin` over what this crate actually
//! provides, `createRequire` refused for the loader it needs and does not
//! have.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search found nothing in
//! `rts-cranelift` about module resolution — deciding what a specifier means
//! is the language's, per this repository's own architecture split, and
//! nothing at the machine layer knows a specifier exists. Inside
//! `rts-core-rwk`, [`rts_core_rwk::entry::modules`] holds `declare_module`/
//! `module_at`, which is the table a specifier resolves THROUGH at import
//! time — it is not a list this module can walk (`module_at` is a private
//! `impl Context` method, and nothing public enumerates every registered
//! specifier), so [`builtinModules`]/[`isBuiltin`] cannot be built by reading
//! it back.
//!
//! # Why this does not hardcode a second list
//!
//! `crates/rts-node-rwk/src/lib.rs`'s `install` already IS the list of what
//! this crate provides — the literal `[("assert", …), ("buffer", …), …]` it
//! registers every specifier from. Writing a second list of the same names
//! here is exactly the duplication `RULE 0b`/this crate's own doc warns
//! about: the two would agree today and silently drift the next time a
//! module is added to one and not the other, which is a worse failure than
//! refusing outright — `isBuiltin` would answer confidently and wrongly.
//!
//! So [`namespace`] takes the list as a parameter instead of restating it.
//! **This is the piece this crate cannot wire itself**: the coordinator,
//! which already owns `lib.rs`'s `install` and assembles this module into
//! it, is the one place that has the true list without copying it — passing
//! `install`'s own names (the same slice `install`'s `for` loop is already
//! iterating) into `module::namespace(context, &names)` costs it nothing it
//! does not already have in hand.
//!
//! # Not implemented, by name
//!
//! `createRequire`, the `Module` class, `require()`, `require.cache`,
//! `Module._resolveFilename`/`_load`/`_nodeModulePaths` — all assume a
//! dynamic runtime loader: something that can compile and run a path
//! discovered only at run time, the way `require('./x')` inside a callback
//! can name a file the compiler never saw. This engine resolves an entire
//! program's imports once, at compile time, into a fixed set — there is no
//! re-entrant resolver for a native to call, and nothing in
//! `rts_core_rwk::entry` exposes one. `register`, `registerHooks` — need the
//! same dynamic loader to actually intercept, plus (for `register`) a
//! dedicated worker thread this engine has no `worker_threads` equivalent
//! for. `findPackageJSON` — a pure filesystem walk in principle, but rooted
//! at "the caller's own module URL", which needs the compiler's per-file
//! import graph to say where a caller's source lives; not reachable from a
//! native entry point. `stripTypeScriptTypes` — needs the TS parser
//! (`rts-parser`), which this crate does not depend on and RULE 0b's own
//! table does not license reaching for from here. `SourceMap`,
//! `findSourceMap`, `getSourceMapsSupport`/`setSourceMapsSupport` — no
//! module here retains a source map or consults one when formatting a
//! crash trace. `syncBuiltinESMExports` — this engine's builtin namespaces
//! are not V8-style live-binding ESM objects with a CJS view to resync.
//! `enableCompileCache`/`flushCompileCache`/`getCompileCacheDir`/
//! `module.constants` — this engine's own build/JIT cache lives in the CLI
//! pipeline, outside anything `rts_core_rwk::entry` reaches.

use rts_core_rwk::entry::{Context, Provided};
use std::sync::OnceLock;

/// The specifiers `install` registers, recorded once when [`namespace`] is
/// built, so [`is_builtin`] (an `extern "C"` function pointer, which cannot
/// capture a slice) has something to read.
static IMPLEMENTED: OnceLock<Vec<String>> = OnceLock::new();

/// The namespace `node:module` is, given the list of specifiers `install`
/// already provides — see the module doc for why that list is a parameter
/// rather than a second copy.
pub fn namespace(context: &mut Context, implemented: &[&str]) -> u64 {
    IMPLEMENTED.get_or_init(|| implemented.iter().map(|name| (*name).to_owned()).collect());
    let members: &[(&str, Provided)] = &[
        ("builtinModules", builtin_modules),
        ("isBuiltin", is_builtin),
    ];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `module.builtinModules` — every name `install` registers, in the bare
/// (unprefixed) spelling `install`'s own list is written in; a caller
/// wanting `"node:fs"` gets that by prefixing, which is what
/// [`is_builtin`] accepts on either side.
extern "C" fn builtin_modules(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let names = IMPLEMENTED.get().cloned().unwrap_or_default();
    rts_core_rwk::entry::with_runtime(|context| {
        let values = names
            .iter()
            .map(|name| rts_core_rwk::entry::make_string(context, name))
            .collect();
        rts_core_rwk::entry::make_array_in(context, values)
    })
}

/// `module.isBuiltin(name)` — true for a name `install` provides, under
/// either spelling. Answers what THIS crate provides, which is honest
/// rather than Node's own answer (true for every core module Node ships,
/// implemented or not) — a name this crate does not yet have is `false`
/// here and an unresolved import there, not a silent `true` for a module
/// that then fails to resolve.
extern "C" fn is_builtin(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = rts_core_rwk::entry::text_of(value) else {
        return rts_core_rwk::entry::boolean_value(false);
    };
    let bare = text.strip_prefix("node:").unwrap_or(&text);
    let found = IMPLEMENTED
        .get()
        .is_some_and(|names| names.iter().any(|name| name == bare));
    rts_core_rwk::entry::boolean_value(found)
}
