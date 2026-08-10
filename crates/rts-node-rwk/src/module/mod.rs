//! `node:module` — what this runtime can import (`builtinModules`/`isBuiltin`),
//! the CommonJS wrapper as a string operation (`Module.wrap`), the nearest
//! `package.json` for a specifier (`findPackageJSON`), and the `SourceMap`
//! class. `createRequire` and the hook/compile-cache families are refused by
//! name below.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search, re-run for this change:
//!
//! - **The machine** (`rts-cranelift`) answers nothing here. Searched
//!   `src/symbols/`, `src/abi/` and `src/mem/` for a specifier, a path or a
//!   source map: deciding what a specifier means is the language's per this
//!   repository's own split, and a source map is a run-time table rather than
//!   anything the compiler emits.
//! - **The host surface** (`rts_core_rwk::entry::modules`, read in full) now
//!   holds `module_at_name`, `module_specifiers` and `forget_module` — the
//!   specifier table, readable from a native. `rts-std-rwk`'s `rts:runtime`
//!   already reaches all three, so this module does NOT hold a second table
//!   or a second resolver; see "Why `createRequire` is still refused".
//! - **Inside this crate**: `url/mod.rs` already worked out how a host module
//!   builds a constructible class (`make_callable` for the constructor,
//!   `make_prototype` for the methods, `prototype` put on the constructor) and
//!   [`source_map`] follows it rather than inventing a second recipe.
//!   `url/fileurl.rs` converts a `file://` URL to a path, but only as a
//!   `Provided` `extern "C"` function over JS values — there is no Rust helper
//!   to call, so [`packages`] goes to the `url` crate (already a dependency)
//!   directly, which is the one implementation both sides ultimately use.
//! - **Nothing found for base64-VLQ.** `node:buffer`'s codec has base64 for
//!   BYTES; a source map's `mappings` is a different alphabet-to-integer
//!   scheme (continuation bit, sign bit, running deltas) over the same 64
//!   characters, so reusing the byte decoder would decode the wrong thing.
//!   [`source_map`] states this at its own definition.
//!
//! # Why this does not hardcode a second list of module names
//!
//! `crates/rts-node-rwk/src/lib.rs`'s `install` already IS the list of what
//! this crate provides — the literal `[("assert", …), ("buffer", …), …]` it
//! registers every specifier from. Writing a second list of the same names
//! here is the duplication `RULE 0b` warns about: the two would agree today
//! and silently drift the next time a module is added to one and not the
//! other, and `isBuiltin` would then answer confidently and wrongly.
//!
//! So [`namespace`] takes the list as a parameter. The coordinator — `lib.rs`,
//! which already owns `install` — is the one place that has the true list
//! without copying it.
//!
//! `docs/reference/node/module.md` §5.1 recommends the opposite: a full
//! canonical Node-25 name list, so `isBuiltin('http3')`-style names Node ships
//! but RTS has not written still answer `true`. That is **rejected here**, and
//! the reason is the honesty floor: a `true` from `isBuiltin` for a module
//! whose import then fails to resolve is a wrong answer that runs, and the
//! program cannot tell it from a working one. A `false` for a module this
//! runtime does not have is exactly what an import of it will do. The
//! divergence it costs is Node's mandatory-`node:`-prefix asymmetry
//! (`isBuiltin('sqlite') === false` in Node): `install` registers every module
//! under both spellings, so both answer `true` here, which is true OF THIS
//! RUNTIME.
//!
//! # `createRequire` is REAL now, and the refusal that stood here was stale
//!
//! The reason recorded — "this engine resolves an entire program's imports
//! once, at compile time; there is no re-entrant resolver for a native to
//! call" — described a tree that has changed. `rts-host-rwk`'s `graph.rs`
//! walks a program's whole relative-import graph and `rts-codegen` emits all
//! of it into ONE compilation, so every file of the program is in the
//! specifier table at run time, keyed by its resolved absolute path, and
//! `entry::module_at_name` reads it. `entry::closure_new` supplies the other
//! half — a callable over a Rust function AND a captured environment — so a
//! `require` can be rooted at a directory. [`require`] holds it, states what
//! it still cannot do (LOAD a file the program was not built with) and refuses
//! `require.cache`/`.main`/`.extensions` by name for reasons of their own.
//!
//! # Not implemented, by name
//!
//! Two kinds, and the difference is deliberate.
//!
//! **Present, and answering `undefined`** — every one of these returns a
//! VALUE in Node, so `undefined` is distinguishable from having worked, and a
//! program fails at the call with something it can test rather than at a
//! property read further away:
//!
//! - `register`, `registerHooks` — a hook only means something if the resolver
//!   consults it, and nothing on the host surface lets a native intercept what
//!   an import resolves to. A registration that never fires is the silent
//!   no-op this crate refuses. `register` additionally wants a dedicated
//!   loader thread.
//! - `stripTypeScriptTypes` — needs the TS parser (`rts-parser`); this crate
//!   does not depend on it and RULE 0b's table does not license adding one
//!   from inside a module.
//! - `enableCompileCache`, `flushCompileCache` — this engine's own artifact
//!   cache lives in the CLI pipeline, outside anything `rts_core_rwk::entry`
//!   reaches.
//! - `getSourceMapsSupport` — nothing here consults a source map when
//!   formatting a trace, so there is no flag to report.
//!
//! **Absent entirely** — because in Node each of these returns `undefined` or
//! nothing, so a present stub would be indistinguishable from success, which
//! is the worst outcome available:
//!
//! - `syncBuiltinESMExports`, `setSourceMapsSupport`, `Module.runMain` — all
//!   `void` in Node.
//! - `getCompileCacheDir`, `findSourceMap` — `undefined` is a legal Node
//!   answer for both ("never enabled", "no map"), so a stub answering it would
//!   be a lie a program cannot detect. Nothing here retains a source map per
//!   module for `findSourceMap` to look one up in.
//! - `module.constants` (`compileCacheStatus`) — its members are numeric codes
//!   this crate does not know, and inventing four numbers is fabricating
//!   values; every consumer of them is refused anyway.
//! - The `Module` class as a CONSTRUCTOR, and every instance member of it
//!   (`exports`, `id`, `filename`, `loaded`, `parent`, `children`, `path`,
//!   `paths`, `isPreloading`, `module.require`) — a `Module` instance is the
//!   record of a file Node LOADED, with a parent, children and a load order.
//!   The specifier table holds namespaces, not load records: nothing here
//!   knows which module required which, so every one of those fields would be
//!   invented.
//! - `module.exports`/`require`/`__filename`/`__dirname` as ambient wrapper
//!   bindings — injecting a binding into every file's scope is the compiler's,
//!   not a host module's.
//! - `Module._cache`/`_extensions`/`_pathCache`/`_resolveFilename`/`_load`/
//!   `_nodeModulePaths` — the same load records, under their legacy names.
//!   `_resolveFilename` and `_nodeModulePaths` are reachable as
//!   `require.resolve` and `require.resolve.paths`, which is where they are.
//! - `SourceMap`'s `lineLengths` option — it only steers Node's own
//!   line-length fallback lookup, which this decoder does not have.
//!
//! [`packages`] and [`source_map`] each name their own gaps too.

mod packages;
mod require;
mod source_map;

use rts_core_rwk::entry::{self, Context, Provided};
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

    // `builtinModules` is a DATA property holding an array, because that is what
    // it is in Node. It was a function here once, so a program reading
    // `builtinModules.length` got a function's arity — a number, plausible, and
    // wrong. The names are the bare (unprefixed) spelling `install`'s own list
    // is written in; a caller wanting `"node:fs"` prefixes it, which is what
    // `is_builtin` accepts on either side.
    //
    // Built ONCE and named on both objects below, rather than per read: the
    // list cannot change after `install` assembled it, a fresh array each read
    // would make `builtinModules === builtinModules` false, and in Node
    // `Module.builtinModules` and `module.builtinModules` are the same array.
    let values = implemented
        .iter()
        .map(|name| entry::make_string(context, name))
        .collect();
    let names = entry::make_array_in(context, values);
    let source_map = source_map::install(context);

    let members: &[(&str, Provided)] = &[
        ("isBuiltin", is_builtin),
        ("findPackageJSON", packages::find_package_json),
        ("createRequire", require::create_require),
        ("wrap", wrap),
        ("register", refused),
        ("registerHooks", refused),
        ("stripTypeScriptTypes", refused),
        ("enableCompileCache", refused),
        ("flushCompileCache", refused),
        ("getSourceMapsSupport", refused),
    ];
    let namespace = entry::make_namespace(context, members);
    entry::put_member(context, namespace, "builtinModules", names);
    entry::put_member(context, namespace, "SourceMap", source_map);

    // `Module` carries the statics a program reaches through it. It is an
    // object rather than a constructible class because constructing one is
    // refused (see the module doc) — a callable that cannot build an instance
    // would be a worse answer than a namespace that never claimed to.
    let statics: &[(&str, Provided)] = &[
        ("isBuiltin", is_builtin),
        ("wrap", wrap),
        ("createRequire", require::create_require),
    ];
    let class = entry::make_namespace(context, statics);
    entry::put_member(context, class, "builtinModules", names);
    entry::put_member(context, class, "SourceMap", source_map);
    entry::put_member(context, namespace, "Module", class);
    // Node's default export of `node:module` is the `Module` class itself, and
    // `import mod from "node:module"` binds the `default` key here — the same
    // wiring `console.rs` uses for its own default instance.
    entry::put_member(context, namespace, "default", class);
    namespace
}

/// `module.isBuiltin(name)` — true for a name `install` provides, under either
/// spelling.
///
/// The test is `string_in` rather than `text_of`: `text_of` is `ToString`, so
/// it answers `"fs"` for an object whose `toString` says so and would report a
/// module for a value that is not a name at all. Asking WHAT something is takes
/// a predicate, not a coercion.
extern "C" fn is_builtin(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::with_runtime(|context| entry::string_in(context, value)) else {
        return entry::boolean_value(false);
    };
    let bare = text.strip_prefix("node:").unwrap_or(&text);
    let found = IMPLEMENTED
        .get()
        .is_some_and(|names| names.iter().any(|name| name == bare));
    entry::boolean_value(found)
}

/// `Module.wrap(script)` — the CommonJS wrapper, as text.
///
/// A pure string operation, which is why it is real here while everything else
/// about CommonJS is refused: it does not load, resolve or run anything, and
/// the five parameter names and their order are the observable contract
/// tooling uses it for.
extern "C" fn wrap(_e: u64, _this: u64, script: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::with_runtime(|context| entry::string_in(context, script)) else {
        return entry::undefined_value();
    };
    let wrapped =
        format!("(function (exports, require, module, __filename, __dirname) {{ {text}\n}});");
    entry::with_runtime(|context| entry::make_string(context, &wrapped))
}

/// The members named in the module doc's first refusal list: present, so a call
/// fails at the call with a value a program can test, and answering `undefined`
/// where Node answers a require function, a promise, a registration handle, a
/// string or an options object.
extern "C" fn refused(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

/// Whether a specifier names a module this runtime provides — the same question
/// [`is_builtin`] answers, for [`packages`], which must not walk the filesystem
/// looking for a `package.json` enclosing `node:fs`.
fn is_provided(specifier: &str) -> bool {
    let bare = specifier.strip_prefix("node:").unwrap_or(specifier);
    IMPLEMENTED
        .get()
        .is_some_and(|names| names.iter().any(|name| name == bare))
}
