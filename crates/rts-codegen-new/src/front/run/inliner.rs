//! CRANELIFT'S INLINING PASS, driven by a size heuristic (`RTS_INLINE=1`).
//!
//! `CRANELIFT_IMPLEMENTATION.md` §3 listed `Context::inline` / `trait Inline` as
//! an API with zero uses in the tree, and argued that Wasmtime's reason for
//! keeping the pass off by default does not transfer: *its* input arrives already
//! inlined by LLVM, so the compile-time cost buys nothing. RTS's input is
//! TypeScript, and nothing has inlined it.
//!
//! ## Why this can run at all
//!
//! Cranelift's pass is embedder-driven BY DESIGN — the module's own docs say it
//! "does not attempt to define heuristics". The embedder must hand it the
//! callee's CLIF body, which means the caller has to already have it. RTS does:
//! `module_jit::populate_module` builds the IR of every function into a
//! `Vec<Pending>` BEFORE anything is machine-compiled, so at the top of
//! [`super::parcompile::compile_and_define`] every body in the program is in
//! hand at once.
//!
//! That also decides where it runs: the callee map is read-only, so the pass
//! goes INSIDE the existing parallel region, next to `ctx.compile`. It costs no
//! serial time.
//!
//! ## What it cannot reach
//!
//! Only functions RTS itself lowered. A `__rtsadp_*` trampoline is Rust-compiled
//! and has no CLIF, so the opaque call boundary §1e.2 measured at 12.2x is NOT
//! what this addresses — §3 says so explicitly, and it is worth repeating here so
//! the pass is not mistaken for that fix. What it does reach is the prelude's own
//! small wrappers and user functions calling each other.
//!
//! ## The heuristic
//!
//! Inline a DIRECT call when the callee's body is at most [`MAX_INSTS`]
//! instructions and is not the caller itself (a self-recursive call would inline
//! forever). `visit_callee: false` — the inlined body is not re-scanned, so one
//! pass cannot cascade. Both are deliberately conservative: this is a
//! measurement, and a heuristic that has to be tuned before it is correct is not
//! one worth measuring yet.

use std::collections::HashMap;
use std::sync::Arc;

use cranelift_codegen::ir;
use cranelift_codegen::inline::{Inline, InlineCommand};

/// Callee size ceiling, in instructions across all blocks. `RTS_INLINE_MAX`
/// overrides so the threshold can be swept without a rebuild.
fn max_insts() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("RTS_INLINE_MAX")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(MAX_INSTS)
    })
}

/// Default ceiling. Small enough that the inlined body is plausibly cheaper than
/// the call sequence it replaces, and that code growth stays bounded.
const MAX_INSTS: usize = 24;

/// Is the pass enabled? ON by default; `RTS_INLINE=0` is the kill switch.
///
/// Measured (release, median of 7): a 5M-call benchmark goes 47 -> 40 ms (15%), a
/// prelude-heavy array/string benchmark 157 -> 154 ms, and a call-free integer
/// loop is unchanged — which is the expected shape, since this only removes CALL
/// overhead. Machine-compile time is unchanged within noise, and the TS suite is
/// byte-identical with it on and off.
pub(super) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_INLINE")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

/// Instructions in `f`, across every block.
fn inst_count(f: &ir::Function) -> usize {
    f.layout
        .blocks()
        .map(|b| f.layout.block_insts(b).count())
        .sum()
}

/// Does `f` contain a `global_value` instruction?
///
/// **This is a hard requirement, not a heuristic.** Cranelift's inliner asserts
/// the callee carries none (`inline.rs:402`: "callee must already be legalized,
/// we shouldn't see any `global_value` instructions when inlining") and PANICS
/// otherwise — it is not a graceful bail. RTS emits `global_value` all over: IC
/// cells (`ic.rs`), AOT string data (`aot_str.rs`) and module globals.
///
/// Found by measurement, not by reading: the default ceiling of 24 instructions
/// happened to exclude every such body, and raising it to 48 crashed the compile
/// on the first program tried. A size ceiling is therefore NOT a safety property
/// here — this check is.
fn has_global_value(f: &ir::Function) -> bool {
    f.layout.blocks().any(|b| {
        f.layout
            .block_insts(b)
            .any(|i| f.dfg.insts[i].opcode() == ir::Opcode::GlobalValue)
    })
}

/// The bodies eligible to be inlined, keyed by the `FuncId` index a caller's
/// `UserExternalName` carries. Built once, shared read-only by every worker.
pub(super) type Bodies = Arc<HashMap<u32, Arc<ir::Function>>>;

/// Collect the small, inlinable bodies out of the IR already built for this
/// program. `funcs` is `(FuncId index, body)` for every pending function.
pub(super) fn collect<'a>(funcs: impl Iterator<Item = (u32, &'a ir::Function)>) -> Bodies {
    let limit = max_insts();
    let mut out: HashMap<u32, Arc<ir::Function>> = HashMap::new();
    for (id, f) in funcs {
        if inst_count(f) <= limit && !has_global_value(f) {
            out.insert(id, Arc::new(f.clone()));
        }
    }
    Arc::new(out)
}

/// The `Inline` implementation: resolve the callee `FuncRef` back to its
/// `FuncId`, and hand Cranelift the body when it is one of the collected small
/// ones.
pub(super) struct SizeInliner {
    bodies: Bodies,
    /// The `FuncId` index of the function being compiled — never inline it into
    /// itself.
    self_id: u32,
    /// Scratch holding the body handed to Cranelift, so the returned `Cow` can
    /// borrow from `self` instead of cloning the whole function per call site.
    current: Option<Arc<ir::Function>>,
}

impl SizeInliner {
    pub(super) fn new(bodies: Bodies, self_id: u32) -> Self {
        Self {
            bodies,
            self_id,
            current: None,
        }
    }
}

impl Inline for SizeInliner {
    fn inline(
        &mut self,
        caller: &ir::Function,
        _call_inst: ir::Inst,
        call_opcode: ir::Opcode,
        callee: ir::FuncRef,
        _call_args: &[ir::Value],
    ) -> InlineCommand<'_> {
        // Only a plain direct `call`. `call_indirect` has no static callee, and
        // `return_call` is the TCO path — inlining a tail call would undo the
        // very thing `CallConv::Tail` exists for.
        if call_opcode != ir::Opcode::Call {
            return InlineCommand::KeepCall;
        }
        let ext = &caller.dfg.ext_funcs[callee];
        let ir::ExternalName::User(nr) = ext.name else {
            return InlineCommand::KeepCall;
        };
        let un = &caller.params.user_named_funcs()[nr];
        // Namespace 0 is a FUNCTION (namespace 1 is data — see
        // `cranelift_module`'s `ModuleRelocTarget`).
        if un.namespace != 0 || un.index == self.self_id {
            return InlineCommand::KeepCall;
        }
        let Some(body) = self.bodies.get(&un.index) else {
            return InlineCommand::KeepCall;
        };
        // A signature mismatch here is a panic inside the pass, so check it
        // rather than trust the id: the caller's imported signature must equal
        // the callee's own.
        if caller.dfg.signatures[ext.signature] != body.signature {
            return InlineCommand::KeepCall;
        }
        self.current = Some(body.clone());
        let f = self.current.as_ref().expect("just set");
        InlineCommand::Inline {
            callee: std::borrow::Cow::Borrowed(f),
            // Do not re-scan the inlined body: one pass, no cascade. Bounds both
            // code growth and compile time while the heuristic is unproven.
            visit_callee: false,
        }
    }
}
