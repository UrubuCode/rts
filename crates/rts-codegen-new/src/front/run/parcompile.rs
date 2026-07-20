//! PARALLEL CRANELIFT COMPILATION — the per-function machine-code phase, run
//! across cores.
//!
//! ## Why this split exists
//!
//! Lowering a function needs `&mut dyn Module` (it declares callees, data and
//! signatures as it goes), so IR construction must stay serial. But
//! `Context::compile` — regalloc, the egraph passes, machine-code emission — is
//! the expensive part and touches nothing shared. Measured on an empty program
//! (which still compiles the reachable prelude): ~155 ms of function bodies plus
//! ~21 ms of thunks, all of it per-function independent work on one core.
//!
//! So the pipeline is:
//!
//! ```text
//! serial    build the clif IR for every function        (needs &mut Module)
//! PARALLEL  ctx.compile(isa)                            (the expensive part)
//! serial    module.define_function_bytes(id, …)         (needs &mut Module)
//! ```
//!
//! This is the split `cranelift_module::Module::define_function_bytes` exists
//! for; the serial phase does exactly what `define_function_with_control_plane`
//! does internally, just with the compile already done.
//!
//! ## Why it cannot change behaviour
//!
//! `ctx.compile` is a pure function of the `Context` and the ISA, and
//! `TargetIsa` is `Send + Sync`. Functions are defined into the module in the
//! ORIGINAL order regardless of which worker compiled them, so module layout is
//! byte-identical to the serial path. Either the same machine code comes out, or
//! compilation fails — there is no third outcome where semantics drift.
//!
//! `RTS_CODEGEN_JOBS=1` forces the serial path (a bisect escape hatch, and the
//! way to A/B this phase with one binary).

use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_module::{FuncId, Module, ModuleReloc};
use rayon::prelude::*;

use std::collections::HashMap;

use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use rts_hir::HirFunc;

use crate::front::error::{FrontResult, Unsupported};

use std::sync::atomic::Ordering;

use super::lower::Lowerer;
use super::sig::FnSig;

/// A function whose IR is built and which is waiting to be machine-compiled.
pub(super) struct Pending {
    pub id: FuncId,
    pub ctx: Context,
    /// Only for the error message — a Cranelift failure must name the function.
    pub name: String,
}

/// The machine code of one compiled function, owned so no borrow of its
/// `Context` outlives the parallel region.
struct Emitted {
    id: FuncId,
    name: String,
    alignment: u64,
    bytes: Vec<u8>,
    relocs: Vec<ModuleReloc>,
}

/// Compile every pending function (in parallel when enabled) and define them
/// into `module` in the given order.
pub(super) fn compile_and_define(
    module: &mut dyn Module,
    pending: Vec<Pending>,
) -> FrontResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    crate::timing::note("fns to machine-compile", pending.len());

    // PARALLEL PHASE. Borrows `module` only immutably (for the ISA) and returns
    // fully owned results, so the mutable borrow below is free to start.
    let emitted: Vec<Emitted> = {
        let isa = module.isa();
        let compile_one = |mut p: Pending| -> FrontResult<Emitted> {
            let res = p
                .ctx
                .compile(isa, &mut ControlPlane::default())
                .map_err(|e| Unsupported::new(format!("compile `{}`: {e:?}", p.name)))?;
            let alignment = res.buffer.alignment as u64;
            let code = p
                .ctx
                .compiled_code()
                .expect("compiled_code present after a successful compile");
            let relocs = code
                .buffer
                .relocs()
                .iter()
                .map(|r| ModuleReloc::from_mach_reloc(r, &p.ctx.func, p.id))
                .collect();
            Ok(Emitted {
                id: p.id,
                name: p.name.clone(),
                alignment,
                bytes: code.buffer.data().to_vec(),
                relocs,
            })
        };
        if jobs() == 1 {
            crate::timing::phase("  machine-compile (serial)", || {
                pending
                    .into_iter()
                    .map(compile_one)
                    .collect::<FrontResult<_>>()
            })?
        } else {
            crate::timing::phase("  machine-compile (parallel)", || {
                pool().install(|| {
                    pending
                        .into_par_iter()
                        .map(compile_one)
                        .collect::<FrontResult<_>>()
                })
            })?
        }
    };

    // SERIAL PHASE. Definition order is the input order, so the module layout
    // does not depend on which worker finished first.
    crate::timing::phase("  define_function_bytes", || {
        for e in &emitted {
            module
                .define_function_bytes(e.id, e.alignment, &e.bytes, &e.relocs)
                .map_err(|err| Unsupported::new(format!("define `{}`: {err}", e.name)))?;
        }
        Ok(())
    })
}

/// A DEDICATED rayon pool whose workers get a big stack.
///
/// `Context::compile` recurses over the function's IR and is stack-hungry. A
/// default rayon worker starts with a far smaller stack than the process main
/// thread, and running the compile on one overflowed it: the unit-test binary
/// died with STATUS_STACK_OVERFLOW (0xc00000fd) on the first parallel run, on
/// functions that compiled fine serially. 32 MiB matches what the deep-IR
/// fixtures need with headroom.
///
/// Using our own pool rather than rayon's global one also keeps this phase from
/// competing with any other rayon user in the process (the `parallel` namespace).
fn pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs())
            .stack_size(32 * 1024 * 1024)
            .thread_name(|i| format!("rts-codegen-{i}"))
            .build()
            .expect("build codegen thread pool")
    })
}

/// Worker count for the machine-compile phase. `RTS_CODEGEN_JOBS` overrides;
/// `1` forces the serial path.
fn jobs() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("RTS_CODEGEN_JOBS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            })
    })
}
/// Lower one function to Cranelift IR and return its `Context`, ready for the
/// machine-compile phase ([`super::parcompile`]). On an `Unsupported` bail the
/// half-built `ctx` is discarded WITHOUT finalizing (an incomplete Cranelift
/// function must never be defined), and the error propagates.
///
/// This is the SERIAL half: lowering declares callees/data/signatures as it
/// goes, so it needs `&mut dyn Module`. Everything after it — regalloc, the
/// egraph passes, emission — touches nothing shared and runs across cores.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_one(
    module: &mut dyn Module,
    id: cranelift_module::FuncId,
    func: &HirFunc,
    sig: &FnSig,
    sigs: &HashMap<String, FnSig>,
    thunks: &HashMap<String, FuncId>,
    ids: &HashMap<String, FuncId>,
    classes: &super::class::ClassTable,
    captures: &HashMap<String, Vec<String>>,
    cells: &HashMap<String, std::collections::HashSet<String>>,
    gcells: &HashMap<String, u32>,
    immutable_gcells: &std::collections::HashSet<u32>,
    gcell_classes: &HashMap<String, String>,
    globalthis_class_refs: &HashMap<String, String>,
    this_class: Option<&str>,
    is_prelude: bool,
    builtins: &HashMap<String, (String, String)>,
    param_classes: &HashMap<String, HashMap<String, String>>,
) -> FrontResult<Pending> {
    let mut ctx = module.make_context();
    ctx.func.signature = sig.to_cranelift(module);

    {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        let cell_locals = cells.get(&func.name).cloned().unwrap_or_default();
        let param_cls = param_classes.get(&func.name).cloned().unwrap_or_default();
        let res = Lowerer::lower_function(
            module,
            &mut fb,
            func,
            sig,
            sigs,
            thunks,
            ids,
            classes,
            captures,
            gcells,
            immutable_gcells,
            gcell_classes,
            globalthis_class_refs,
            this_class,
            is_prelude,
            builtins,
            cell_locals,
            param_cls,
        );
        match res {
            Ok(()) => fb.finalize(),
            Err(e) => {
                // drop the builder/ctx without defining; clear and bail — naming
                // WHICH function failed (a prelude bail is otherwise untraceable).
                module.clear_context(&mut ctx);
                return Err(Unsupported::new(format!("in fn `{}`: {e}", func.name)));
            }
        }
    }

    // `rts ir`: print USER functions only — the embedded stdlib prelude would bury
    // the program under hundreds of identical bundle fns.
    if !is_prelude && super::module_jit::DUMP_IR.load(Ordering::Relaxed) {
        eprintln!("--- fn {} ---", func.name);
        eprintln!("{}", ctx.func.display());
    }

    Ok(Pending {
        id,
        ctx,
        name: func.name.clone(),
    })
}
