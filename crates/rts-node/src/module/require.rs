//! `module.createRequire(filename)` — a `require` rooted at a directory.
//!
//! # Why this is real, and what the refusal that stood here was wrong about
//!
//! It was refused for "no dynamic runtime loader", and then for "a native
//! cannot close over the filename it was rooted at". Both were wrong, and the
//! second was wrong about this crate's own code:
//!
//! - `rts-host`'s `graph.rs` walks a program's whole relative-import graph
//!   and emits every file into ONE compilation, so each module of the program
//!   is in `rts-core`'s specifier table at run time, keyed by its resolved
//!   absolute path — `entry::module_at_name` reads it, and `rts-std`'s
//!   `rts:runtime` already does exactly that.
//! - `entry::closure_new` mints a callable over a Rust function AND an
//!   environment value, which the native receives as its first parameter.
//!   `perf_hooks/timerify.rs` already does this to carry a wrapped function
//!   into its wrapper; the same mechanism carries a base directory here, so
//!   two `createRequire` calls answer two functions that resolve `'./x'`
//!   differently. `entry::make_callable` (environment fixed to `undefined`)
//!   was the wrong tool, not a missing capability.
//!
//! # What it cannot do, and why that is a refusal rather than a fallback
//!
//! It cannot LOAD. A file outside the program's own graph would have to be
//! compiled into a region of its own, and its exports would be cells this
//! program cannot touch — the wall `node:vm`, `worker_threads` and
//! `rts:runtime`'s own refused `reload` all meet. So `require` answers what
//! the program was built with, and `undefined` for anything else, rather than
//! a namespace whose every value is unusable.
//!
//! # Why no extension is guessed
//!
//! `graph.rs` refuses to: `./x` is `./x` and `./x.ts` is `./x.ts`, because a
//! resolver that tries `.ts`, then `.js`, then `/index.ts` picks a file the
//! program did not name. The table's keys are what that loader produced, so
//! resolving through a second, more generous algorithm here would answer a key
//! nothing registered — or, worse, a different file's. A specifier is spelled
//! the way the program's own `import` spells it.
//!
//! # Not implemented, by name
//!
//! - **`require.cache`.** Its point is that deleting an entry re-executes the
//!   module on the next `require`, which needs the loading this cannot do. An
//!   object that accepted the deletion and changed nothing is the silent no-op
//!   this crate refuses. (`rts:runtime`'s `module.delete` does the honest half
//!   — forget the name — and says so.)
//! - **`require.main`.** Nothing here knows which module is the entry point:
//!   the specifier table is a flat map and the host does not mark one. A guess
//!   would make `require.main === module` — the idiomatic "was I run
//!   directly" test — answer for the wrong file.
//! - **`require.extensions`.** Deprecated since v0.10.6, and inert by
//!   definition; present-and-inert is indistinguishable from working.
//! - **`MODULE_NOT_FOUND`.** No function on this crate's host surface raises a
//!   catchable exception (`entry::throw` ends the program instead of unwinding
//!   to a handler), so an unresolvable specifier answers `undefined` from
//!   `require` and from `require.resolve`.

use std::path::PathBuf;

use rts_core::entry;

/// Where the base directory lives inside the closure's environment.
///
/// An array rather than an object, for `timerify.rs`'s reason: the environment
/// is read back with the ambient `get_indexed`, which needs no interned key.
const BASE: f64 = 0.0;

/// `module.createRequire(filename)`.
///
/// `filename` is a file, not a directory — Node's argument is the module's own
/// path or URL — and the FILE is what is captured, because that is what the
/// host's resolver takes as a referrer. It used to be the parent directory,
/// beside a copy of the loader's join-and-canonicalise rule; see [`resolved`]
/// for what that copy cost the day the loader's rule changed.
pub(super) extern "C" fn create_require(
    _e: u64,
    _this: u64,
    filename: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(text) = entry::with_runtime(|context| entry::string_in(context, filename)) else {
        return entry::undefined_value();
    };
    let Some(path) = super::packages::base_path(&text) else {
        return entry::undefined_value();
    };
    // Node throws `ERR_INVALID_ARG_VALUE` for a relative path, because a
    // `require` rooted at one would resolve against whatever the working
    // directory happened to be at call time.
    if !path.is_absolute() {
        return entry::undefined_value();
    }
    let base = path.display().to_string();

    let environment = entry::with_runtime(|context| {
        let held = entry::make_string(context, &base);
        entry::make_array_in(context, vec![held])
    });
    // Ambient, and therefore outside the borrow above — `closure_new` takes its
    // own. Three closures over the SAME environment, so `require.resolve` and
    // `require` cannot come to disagree about where they are rooted.
    let require = entry::closure_new(call as *const () as usize as i64, environment);
    let resolve = entry::closure_new(resolve as *const () as usize as i64, environment);
    let paths = entry::closure_new(resolve_paths as *const () as usize as i64, environment);
    entry::with_runtime(|context| {
        entry::put_member(context, resolve, "paths", paths);
        entry::put_member(context, require, "resolve", resolve);
        require
    })
}

/// `require(id)` — the namespace of a module the program was built with.
extern "C" fn call(environment: u64, _this: u64, id: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(key) = resolved(environment, id) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| entry::module_at_name(context, &key))
}

/// `require.resolve(request)` — the name the table knows a module by, without
/// reading it.
///
/// `undefined` for a specifier the table does not hold, rather than the path it
/// would have had: answering a path for a module that is not there is what
/// makes a caller pass it on to something that then finds nothing.
extern "C" fn resolve(environment: u64, _this: u64, request: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(key) = resolved(environment, request) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        match entry::module_at_name(context, &key) == entry::undefined_in(context) {
            true => entry::undefined_in(context),
            false => entry::make_string(context, &key),
        }
    })
}

/// `require.resolve.paths(request)` — the `node_modules` directories a bare
/// specifier would be looked for in, or `null` for a module the host provides.
///
/// Real without any loader: it is a pure function of the base directory, and
/// the answer is what a search WOULD look at rather than what it found.
extern "C" fn resolve_paths(
    environment: u64,
    _this: u64,
    request: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(text) = entry::with_runtime(|context| entry::string_in(context, request)) else {
        return entry::undefined_value();
    };
    if text.starts_with("node:") || super::is_provided(&text) {
        return entry::null_value();
    }
    let Some(base) = base_of(environment) else {
        return entry::undefined_value();
    };
    let mut directories = Vec::new();
    // The captured value is the referrer FILE, so the search starts at its
    // directory — Node's `require.resolve.paths` is rooted where the module is.
    let mut here = base.parent();
    while let Some(directory) = here {
        // A `node_modules` directory does not nest inside itself in a lookup —
        // Node skips one it is already inside rather than proposing
        // `node_modules/node_modules`.
        if directory.file_name().is_some_and(|name| name == "node_modules") {
            here = directory.parent();
            continue;
        }
        directories.push(directory.join("node_modules").display().to_string());
        here = directory.parent();
    }
    entry::with_runtime(|context| {
        let values = directories
            .iter()
            .map(|path| entry::make_string(context, path))
            .collect();
        entry::make_array_in(context, values)
    })
}

/// The base directory a closure was rooted at.
fn base_of(environment: u64) -> Option<PathBuf> {
    let held = entry::get_indexed(environment, entry::make_number(BASE));
    let text = entry::with_runtime(|context| entry::string_in(context, held))?;
    Some(PathBuf::from(text))
}

/// The specifier table's key for a request, from a rooted closure.
///
/// One function so that `require` and `require.resolve` cannot answer two
/// different things — the divergence Node itself documents as the invariant
/// between them.
///
/// # Why it asks the host rather than working the path out
///
/// It used to join the request onto the captured directory and canonicalise it,
/// with a comment saying it did so "exactly as `graph.rs` canonicalises". That
/// is a rule written in two places, and the second one stopped agreeing the day
/// the loader began stripping Windows's verbatim `\?` prefix: this side
/// produced a key nothing had registered, and `require` answered `undefined` —
/// which is precisely what the comment existed to prevent, and could not,
/// because a promise to stay in step is not a mechanism for staying in step.
///
/// `rts_core::entry::resolve_specifier` is the loader's own answer, reached
/// through the hook a dynamic `import()` already resolves through. Now there is
/// one rule and one place it lives.
fn resolved(environment: u64, request: u64) -> Option<String> {
    let text = entry::with_runtime(|context| entry::string_in(context, request))?;
    if text.starts_with("node:") || super::is_provided(&text) {
        return Some(text);
    }
    let from = base_of(environment)?.display().to_string();
    // The host's answer, and the text as written when it has none: a bare
    // specifier is not a path, and the loader leaves those alone too.
    Some(
        entry::with_runtime(|context| entry::resolve_specifier(context, &from, &text))
            .unwrap_or(text),
    )
}
