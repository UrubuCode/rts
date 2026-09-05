//! The archive `rts compile --embed-compiler` links instead of `rts-runtime`'s.
//!
//! # The problem this exists to close
//!
//! An AOT binary from plain `rts compile` links `rts-runtime`, which installs
//! no evaluator — `rts-host`'s own README states the gap plainly: *"eval, new
//! Function and vm.runInNewContext raise there where they work here"*. That is
//! the right default for a program that compiles ahead of time and never
//! touches source again. It is the wrong one for a program whose whole job is
//! to compile source it did not ship with — a browser, running a page's own
//! `<script>` tags, exactly as `examples/claude-browser.ts` does today under
//! `rts run`. Shipping that as a `.exe` needs the compiler inside it, the way
//! Electron ships V8 inside itself rather than asking the OS for a browser.
//!
//! # Why this depends on `rts-runtime-boot` and explicitly NOT on `rts-runtime`
//!
//! It depended on `rts-runtime` first, to reuse its startup sequence without
//! a second copy of it, and the archive that produced compiled and linked
//! with no error — then silently ran the DEFAULT sequence, installing no
//! compiler at all, so `eval` still answered the ordinary refusal. Measured,
//! not assumed: the wiring was checked field-by-field, and `extra` — the
//! parameter carrying `install_compiler` — arrived as `None` in the running
//! process despite this crate's own `main` passing `Some(..)`.
//!
//! The cause is a property of a `staticlib` build neither crate's own code
//! controls: a `#[unsafe(no_mangle)]` item is bundled into a dependent's
//! archive UNCONDITIONALLY once the dependency is reached at all — not only
//! the items the dependent's code actually calls. `rts-runtime` defines
//! `#[unsafe(no_mangle)] fn main`; reaching `rts-runtime` for its startup
//! sequence therefore ALSO bundled that `main`, unreferenced by this crate's
//! code but present regardless. The resulting archive carried two
//! definitions of the linker's most special name, and the linker resolved
//! the collision by keeping one — `rts-runtime`'s.
//!
//! `rts-runtime-boot` is the fix: the startup sequence (`run`), with no
//! `main` of its own, that BOTH `rts-runtime` and this crate depend on
//! instead of on each other. Neither can bundle the other's entry point,
//! because neither reaches the other at all.
//!
//! # Why this crate holds almost no code
//!
//! This crate's `main` is two lines: run `rts-runtime-boot`'s sequence with
//! ONE extra registration — `rts_host::install_compiler`, the same six hooks
//! `rts-host`'s own `run_region` wires for the JIT. A page `<script>`
//! compiled through this hook gets the running program's own singleton
//! numbering, property keys and literal table, not a lookalike compiler
//! bolted on beside them.
//!
//! # The cut, stated rather than discovered
//!
//! What this buys: `eval`, `new Function`, a page's scoped `<script>` eval
//! (`rts-dom-bridge::DomScope::run`), and `vm.runInNewContext`/`runInContext`
//! and their `node:vm` siblings, all working inside the compiled `.exe`
//! exactly as they do under `rts run`.
//!
//! What it does NOT buy: a dynamic `import()` of a file the compiler never
//! saw. `rts_core::entry::module_import`'s own doc is explicit that it reads
//! an already-registered module rather than loading one — *"a rejected
//! promise naming it, not a file read"* — and installing a compiler does not
//! change what that entry point does. Pre-compiling a KNOWN page's scripts
//! into the same binary at build time, so their `import()` targets are
//! already in that table, is the complementary and DIFFERENT lot
//! (`rts compile --html`, `aot-scripts-de-pagina`) — not duplicated here.
//!
//! # The size this costs
//!
//! Linking `rts-codegen` and `rts-cranelift`'s front end and placement code
//! into the binary in addition to everything the default archive already
//! carries — a compiler is not a small thing to carry, and the whole point of
//! the DEFAULT archive staying as it is is that most compiled programs never
//! need one.

/// The C entry point, for an object file whose `main` a linker was told to
/// resolve against THIS archive instead of `rts-runtime`'s.
///
/// # Safety
///
/// Called once, by the C runtime, with the platform's own `argc`/`argv`
/// convention — the same contract [`rts_runtime_boot::run`] documents, which
/// this is the same sequence as, plus one registration. See this crate's own
/// module doc for what that registration is and why `rts-runtime-boot`
/// exists rather than this crate depending on `rts-runtime` for it.
#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const i8) -> i32 {
    rts_runtime_boot::run(argc, argv, Some(rts_host::install_compiler))
}
