//! What `rts compile --embed-compiler` needs from this crate: the six hooks
//! `run_region` wires for the JIT, reachable as ONE public function so a
//! second archive (`rts-runtime-jit`) installs the SAME capability rather
//! than a smaller lookalike of it.
//!
//! # Why this does not compile, link and run a binary
//!
//! The same reason `aot_object.rs` states for its own siblings: that needs a
//! system linker and a runtime archive, and `cargo test -p rts-host` builds
//! neither — `rts-runtime-jit`'s staticlib exists only after its own
//! `cargo build`, exactly as `rts-runtime`'s does. The end-to-end claim —
//! `eval("1+2")` inside a page `<script>`, compiled ahead of time and RUN —
//! is made where `aot_object.rs` already points: the AOT smoke in
//! `.github/workflows/build-artifacts.yml`, over `tests/aot/claude-pagina-eval.ts`
//! and its `.html` fixture, the same way that smoke already compiles, links
//! and runs `tests/aot/graph.ts` and diffs it against a JIT run.
//!
//! What CAN be asserted here, without a linker, is that `install_compiler`
//! wires every one of the six hooks `run_region` used to wire inline before
//! this crate had one function for it — the wiring an edit that dropped one
//! would get wrong SILENTLY, since nothing on this side of the crate boundary
//! calls it to notice, and a missing hook fails only far away, as an AOT
//! binary's `eval` answering the refusal the default archive states rather
//! than a value.

use rts_core::Kinds;
use rts_core::entry::Context;
use rts_core::heap::Region;
use rts_core::value::Singletons;

/// A `Context` with nothing installed yet — enough to ask each hook whether
/// [`rts_host::install_compiler`] set it, without running a program through
/// it. The singleton and kind numbers are arbitrary: nothing here reads a
/// value, so nothing needs them to agree with any compilation's.
fn bare_context() -> Context {
    let singletons = Singletons { undefined: 0, null: 1, hole: 2 };
    let kinds = Kinds { symbol: 3, bigint: 4 };
    Context::over(singletons, kinds, Region::with_capacity(1 << 10))
}

#[test]
fn install_compiler_wires_every_hook_run_region_used_to_wire_inline() {
    let mut context = bare_context();
    assert!(context.function_compiler.is_none(), "nothing installed yet");
    assert!(context.source_parser.is_none(), "nothing installed yet");
    assert!(context.eval_compiler.is_none(), "nothing installed yet");
    assert!(context.eval_compiler_with_receiver.is_none(), "nothing installed yet");
    assert!(context.evaluator.is_none(), "nothing installed yet");
    assert!(context.resolver.is_none(), "nothing installed yet");

    rts_host::install_compiler(&mut context);

    assert!(
        context.function_compiler.is_some(),
        "`new Function` needs a function compiler — this is what an AOT binary \
         compiled with --embed-compiler installs in place of the default \
         archive's refusal"
    );
    assert!(
        context.source_parser.is_some(),
        "eval's well-formedness check needs a parser"
    );
    assert!(
        context.eval_compiler.is_some(),
        "an indirect `eval` needs a scoped compiler"
    );
    assert!(
        context.eval_compiler_with_receiver.is_some(),
        "a page `<script>` (`rts-dom-bridge::DomScope::run`) and `vm.runInContext` \
         both call through this one — it is the hook the bug this whole lot \
         exists for was filed against"
    );
    assert!(
        context.evaluator.is_some(),
        "`vm.runInNewContext` needs a whole-program evaluator"
    );
    assert!(
        context.resolver.is_some(),
        "a dynamic `import()`/`require()` needs to answer what a specifier NAMES \
         even when it is asked from source the compiler placed at run time"
    );
}
