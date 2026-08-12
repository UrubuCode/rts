//! Where compiled code goes.
//!
//! Two destinations — executable memory now, an object file to link later — and
//! one pipeline. They are not two compilers: everything before this module is
//! identical for both, and the difference is entirely in what happens to the
//! bytes at the end. Treating them as two paths is how the two come to disagree
//! about what a program means.
//!
//! # Why a module is what unblocks the rest
//!
//! Lowering can emit anything that stays inside one function. The moment a
//! program calls something, allocates, throws or awaits, it names something that
//! is not in the function — and naming things outside is what a module is for.
//! That is why the refusals in [`crate::lower`] all point here.
//!
//! # Declaring once
//!
//! A function is declared once and referenced by the identifier that declaration
//! returned. This matters more than it sounds: the code generator's own
//! declaration is keyed by name, allocates a string, and probes a table, so a
//! program that re-declares its callee at every call site pays that per site.
//! [`Declarations`] is the cache that makes it per callee instead.

mod declare;
mod destination;
mod hosted;

pub use declare::{Declarations, FunctionRefs, data_ref, func_ref};
pub use destination::{executable_memory, executable_memory_calling, object_file};
pub use hosted::{InMemory, Placing, Visibility, place_in_memory, place_in_object};

use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa, TargetIsa};
use cranelift_module::{
    DataId, FuncId as MachineFuncId, Linkage, Module, ModuleDeclarations, ModuleError, ModuleReloc,
};

use crate::ir::{FuncId, FuncRegistry, Function};
use crate::lower::{LowerError, machine_signature};

/// Why a program could not be placed into a module.
#[derive(Debug)]
pub enum TargetError {
    /// Lowering refused.
    Lower(LowerError),
    /// The module or the code generator refused.
    ///
    /// Kept as the underlying error rather than flattened to a string: what
    /// failed is often the only thing that says which of several plausible
    /// causes it was.
    Module(ModuleError),
    /// A function was defined without being declared.
    ///
    /// Declaring is what produces the identifier a call site refers to, so
    /// defining first means some call site is already naming something that does
    /// not exist.
    UndeclaredFunction(FuncId),
    /// The code generator's `Context::compile` refused a lowered function.
    ///
    /// Kept apart from [`TargetError::Module`]: that variant is what the module
    /// layer (declaring, defining, linking) refuses, and this is regalloc,
    /// verification or emission refusing what `lower` already produced. Carries
    /// a formatted message rather than the borrowed `CompileError` itself — the
    /// borrow is tied to the `Context` the parallel compile phase drops as soon
    /// as each function is done with it.
    Compile(String),
}

impl From<LowerError> for TargetError {
    fn from(error: LowerError) -> Self {
        TargetError::Lower(error)
    }
}

impl From<ModuleError> for TargetError {
    fn from(error: ModuleError) -> Self {
        TargetError::Module(error)
    }
}

/// One compilation, going to one destination.
///
/// Borrows the module rather than owning it, because the two destinations own
/// very different things — executable memory that must outlive what runs in it,
/// or a buffer that becomes a file — and pretending otherwise would put that
/// difference back into every caller.
pub struct MachineModule<'a> {
    module: &'a mut dyn Module,
    declarations: Declarations,
    /// The module's identifier for each runtime entry point.
    ///
    /// Filled by the first pre-pass and read-only afterwards. Optional only
    /// because declaring can fail and [`MachineModule::new`] cannot, so it is
    /// done on the first compilation rather than at construction.
    imports: Option<crate::symbols::EntryImports>,
    /// Which entry points the code compiled so far names.
    entries: crate::symbols::EntryTable,
    heap: Option<crate::mem::RegionBases>,
    /// Where this heap's symbolic base was declared, once it has been.
    ///
    /// Optional for the same reason `imports` is: declaring can fail and
    /// [`MachineModule::new`] cannot, so it happens on the first compilation
    /// rather than at construction — and only at all when [`Self::heap`] is
    /// `Some`, mirroring `imports`'s own guard.
    heap_imports: Option<crate::mem::HeapImports>,
    faults: std::collections::HashMap<FuncId, crate::fault::FaultTable>,
    positions: std::collections::HashMap<FuncId, crate::observe::PositionMap>,
    /// What each function is called and how much code it became.
    ///
    /// Kept so that placing a function in a map does not ask a caller to restate
    /// either. A length restated by hand is a length that can be wrong, and a
    /// wrong length names the wrong function for every address past the end.
    emitted: std::collections::HashMap<FuncId, (String, usize)>,
    call_conv: CallConv,
}

impl<'a> MachineModule<'a> {
    /// Prepares a compilation against a module.
    pub fn new(module: &'a mut dyn Module) -> Self {
        let call_conv = module.isa().default_call_conv();
        Self {
            module,
            declarations: Declarations::new(),
            imports: None,
            entries: crate::symbols::EntryTable::new(),
            heap: None,
            heap_imports: None,
            faults: std::collections::HashMap::new(),
            positions: std::collections::HashMap::new(),
            emitted: std::collections::HashMap::new(),
            call_conv,
        }
    }

    /// The convention this destination uses for anything crossing a boundary.
    pub fn call_conv(&self) -> CallConv {
        self.call_conv
    }

    /// What has been declared so far.
    pub fn declarations(&self) -> &Declarations {
        &self.declarations
    }

    /// Where a compiled function can stop, and where each stop came from.
    ///
    /// Empty until the function is defined, because it is read out of what was
    /// compiled rather than predicted before compiling.
    pub fn faults(&self, id: FuncId) -> Option<&crate::fault::FaultTable> {
        self.faults.get(&id)
    }

    /// Hands over what this compilation learned about its functions.
    ///
    /// Consumes the compilation, which is the point: what comes back is only
    /// usable once the destination has been finalized, and finalizing is not
    /// possible while a compilation is still holding it.
    pub fn into_placements(self) -> Placements {
        Placements {
            emitted: self.emitted,
            positions: self.positions,
        }
    }

    /// Where each run of a compiled function's code came from.
    ///
    /// The answer a profiler needs. Empty until the function is defined, and
    /// empty afterwards if nothing said where anything came from — which is a
    /// truthful empty rather than a silent one.
    pub fn positions(&self, id: FuncId) -> Option<&crate::observe::PositionMap> {
        self.positions.get(&id)
    }

    /// Which runtime entry points this compilation's code reaches for.
    ///
    /// A structural fact, and cheaper to check than arranging to observe a side
    /// effect. It is no longer the same thing as which were *declared* — all
    /// seven are, before anything is lowered, so that lowering never assigns an
    /// identifier — and [`crate::symbols::EntryTable`] says why that trade was
    /// made and what it costs.
    pub fn entries(&self) -> &crate::symbols::EntryTable {
        &self.entries
    }

    /// Declares a function so that call sites can name it.
    ///
    /// Declaring is separate from defining because a program can call something
    /// before it is built — including itself. Requiring definition first would
    /// make recursion unexpressible.
    pub fn declare(
        &mut self,
        id: FuncId,
        name: &str,
        linkage: Linkage,
        funcs: &FuncRegistry,
    ) -> Result<(), TargetError> {
        let signature = funcs
            .signature_of(id)
            .ok_or(TargetError::UndeclaredFunction(id))?;
        let lowered = machine_signature(signature, self.call_conv);
        let declared = self.module.declare_function(name, linkage, &lowered)?;
        self.declarations.record(id, declared, lowered);
        self.emitted.insert(id, (name.to_owned(), 0));
        Ok(())
    }

    /// Gives this compilation a heap to read, write and allocate in.
    ///
    /// Optional because a compilation that never touches an object needs none,
    /// and because how a reference becomes an address is a property of the heap
    /// rather than of any function compiled against it.
    pub fn with_heap(mut self, heap: crate::mem::RegionBases) -> Self {
        self.heap = Some(heap);
        self
    }

    /// Compiles a single function body into the module.
    ///
    /// Convenience over [`prepare`](Self::prepare) +
    /// [`compile_and_define`](Self::compile_and_define) for a caller with only
    /// one function — every existing caller but the batch placer in
    /// `target::hosted`. One function is always under
    /// [`PARALLEL_THRESHOLD`], so this never pays for a thread pool.
    pub fn define(
        &mut self,
        id: FuncId,
        func: &Function,
        funcs: &FuncRegistry,
        types: &crate::types::TypeRegistry,
    ) -> Result<(), TargetError> {
        let prepared = self.prepare(id, func, funcs, types)?;
        self.compile_and_define(vec![prepared])
    }

    /// Compiles a whole program: everything that assigns an identifier first,
    /// then every body lowered and machine-compiled at once, then the
    /// definitions replayed in order.
    ///
    /// # The three phases, and why the boundaries are where they are
    ///
    /// **Serial pre-pass** ([`plan`](Self::plan)). Every identifier this
    /// compilation will use is handed out here, in program order: the seven
    /// runtime entry points, the machine signature of every indirect-call shape,
    /// and one `DataId` per cache site of every function. Nothing about any of
    /// them depends on lowering — the entry-point set is closed and its
    /// signatures are program-independent, a shape is a pure function of the
    /// registry, and a function's cache count is known before its body is
    /// touched. Callees were already declared before this by the caller, because
    /// a program whose functions call each other needs all of them named before
    /// any is defined.
    ///
    /// **Parallel phase.** Lowering *and* `Context::compile`, per function.
    /// Lowering used to be serial because it declared what it reached for as it
    /// went; now it cannot declare anything at all — [`crate::lower::Outside`]
    /// has no `&mut` in it — so it is a pure function of shared, `Sync` state
    /// and one body. This is the change that matters: on realistic bodies our
    /// lowering was the majority of compile time (`examples/phase_split.rs`:
    /// 54% at 32 instructions per function) and all of it was on one thread.
    ///
    /// **Serial replay.** `define_function_bytes`, the fault table, the position
    /// map — all reading compiled bytes back into the module, in the original
    /// order regardless of which worker finished first, so the module's layout
    /// and everything derived from it is identical to compiling serially.
    ///
    /// A batch under [`PARALLEL_THRESHOLD`] skips the pool entirely.
    pub fn compile_all(
        &mut self,
        bodies: &[(FuncId, &Function)],
        funcs: &FuncRegistry,
        types: &crate::types::TypeRegistry,
    ) -> Result<(), TargetError> {
        let planned = {
            let timing = crate::probe::Phase::start("plan");
            let planned = self.plan(bodies, funcs)?;
            timing.over(planned.len());
            planned
        };
        if planned.is_empty() {
            return Ok(());
        }
        let count = planned.len();

        let results = {
            let _timing = crate::probe::Phase::start("lower+compile");
            let shared = self.shared(types);
            let isa = self.module.isa();
            // Lower AND compile, per function, in one pass over the batch. The
            // two used to be separated by a thread boundary because only the
            // second could cross it; nothing separates them now.
            // Summed per stage across whatever thread ran it, because the wall
            // clock of the batch cannot separate them: the two run interleaved
            // on N workers and only their totals are comparable to each other.
            // The sum exceeds the wall clock by the parallelism, which is the
            // point — it is what says whether our lowering or the code
            // generator is the majority of the CPU spent here.
            let lowering = std::sync::atomic::AtomicU64::new(0);
            let compiling = std::sync::atomic::AtomicU64::new(0);
            let work = |one| -> Result<Emitted, TargetError> {
                let at = std::time::Instant::now();
                let prepared = lower_one(&shared, one)?;
                let lowered_at = std::time::Instant::now();
                let emitted = compile_one(isa, prepared)?;
                lowering.fetch_add(
                    (lowered_at - at).as_nanos() as u64,
                    std::sync::atomic::Ordering::Relaxed,
                );
                compiling.fetch_add(
                    lowered_at.elapsed().as_nanos() as u64,
                    std::sync::atomic::Ordering::Relaxed,
                );
                Ok(emitted)
            };

            let results = if planned.len() < PARALLEL_THRESHOLD {
                planned
                    .into_iter()
                    .map(work)
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                use rayon::prelude::*;
                pool().install(|| {
                    planned
                        .into_par_iter()
                        .map(work)
                        .collect::<Result<Vec<_>, _>>()
                })?
            };
            // Two clock reads per function, kept unconditional: at ~40 ns each
            // they are a five-figure fraction of a phase measured in tens of
            // milliseconds, and gating them would mean two spellings of this
            // loop — the second of which is the one that stops being exercised.
            let nanos = |sum: &std::sync::atomic::AtomicU64| {
                sum.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6
            };
            if std::env::var_os("RTS_TIMING").is_some() {
                eprintln!(
                    "rts-timing   cpu lowering {:>8.3} ms, machine-compile {:>8.3} ms \
                     (codegen pass {:>8.3} ms), over {count}",
                    nanos(&lowering),
                    nanos(&compiling),
                    CODEGEN_NANOS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e6,
                );
            }
            results
        };

        let timing = crate::probe::Phase::start("define");
        let defined = self.define_all(results);
        timing.over(count);
        defined
    }

    /// Lowers one function body to the code generator's IR, ready for machine
    /// compilation.
    ///
    /// The one-function shape of [`compile_all`](Self::compile_all): it runs the
    /// same serial pre-pass over that single body and then lowers it here, on
    /// the calling thread. Kept because a caller with one function has nothing
    /// to parallelise, and because it is what
    /// [`compile_and_define`](Self::compile_and_define) — and the determinism
    /// test — are written against.
    pub fn prepare(
        &mut self,
        id: FuncId,
        func: &Function,
        funcs: &FuncRegistry,
        types: &crate::types::TypeRegistry,
    ) -> Result<Prepared, TargetError> {
        let mut planned = self.plan(&[(id, func)], funcs)?;
        let one = planned.pop().ok_or(TargetError::UndeclaredFunction(id))?;
        lower_one(&self.shared(types), one)
    }

    /// Machine-compiles a batch of [`Prepared`] functions and defines them
    /// into the module.
    ///
    /// `Context::compile` — regalloc, verification, machine-code emission — is a
    /// pure function of one `Context` and the ISA, which is what makes running a
    /// batch of them on the pool sound. The definitions that follow are replayed
    /// in `prepared`'s original order.
    pub fn compile_and_define(&mut self, prepared: Vec<Prepared>) -> Result<(), TargetError> {
        if prepared.is_empty() {
            return Ok(());
        }

        // Borrows `self.module` only immutably, for the ISA — `TargetIsa` is
        // `Send + Sync` — so the mutable borrow the serial phase below needs is
        // free to start the moment this ends.
        let isa = self.module.isa();
        let emit = move |p: Prepared| compile_one(isa, p);

        let results: Vec<Emitted> = if prepared.len() < PARALLEL_THRESHOLD {
            prepared.into_iter().map(emit).collect::<Result<_, _>>()?
        } else {
            use rayon::prelude::*;
            pool().install(|| {
                prepared
                    .into_par_iter()
                    .map(emit)
                    .collect::<Result<Vec<_>, _>>()
            })?
        };

        self.define_all(results)
    }

    /// The serial pre-pass: everything that assigns an identifier, in program
    /// order.
    ///
    /// # Why this is the only place a number is handed out
    ///
    /// Rule 13 (`rts-cranelift/README.md`) requires two compilations of one
    /// program to produce the same thing. An identifier assigned on a worker
    /// would be assigned in whatever order the scheduler happened to run, and a
    /// mutex would fix the race while leaving the order — which is the part that
    /// is observable in the artefact. So there is no lock anywhere in this
    /// crate's compile path: the ordering is a property of this loop instead.
    fn plan<'b>(
        &mut self,
        bodies: &[(FuncId, &'b Function)],
        funcs: &FuncRegistry,
    ) -> Result<Vec<Planned<'b>>, TargetError> {
        if self.imports.is_none() {
            self.imports = Some(crate::symbols::EntryImports::declare_all(self.module)?);
        }
        // Same guard, same reason: a heap's symbolic base — if it has one —
        // must be declared before any function that reads it is lowered, and
        // exactly once regardless of how many batches this compilation runs.
        if self.heap_imports.is_none()
            && let Some(bases) = &self.heap
        {
            self.heap_imports = Some(crate::mem::HeapImports::declare(self.module, bases)?);
        }

        let mut planned = Vec::with_capacity(bodies.len());
        for &(id, body) in bodies {
            let declared = self
                .declarations
                .machine_id(id)
                .ok_or(TargetError::UndeclaredFunction(id))?;
            let name = self
                .emitted
                .get(&id)
                .map(|(name, _)| name.clone())
                .ok_or(TargetError::UndeclaredFunction(id))?;

            self.record_shapes(body, funcs);
            let caches = self.declare_caches(id, body)?;

            planned.push(Planned {
                id,
                declared,
                name,
                body,
                caches,
            });
        }
        Ok(planned)
    }

    /// Everything lowering may read, gathered once.
    ///
    /// A separate structure rather than `&self` because `self` holds
    /// `&mut dyn Module`, which is neither `Send` nor `Sync` — and the point of
    /// the change is that lowering does not need it.
    fn shared<'s>(&'s self, types: &'s crate::types::TypeRegistry) -> Shared<'s> {
        Shared {
            machine: self.module.declarations(),
            declarations: &self.declarations,
            imports: self
                .imports
                .as_ref()
                .expect("the pre-pass declared the entry points"),
            heap: self.heap.as_ref(),
            heap_imports: self.heap_imports.as_ref(),
            types,
            call_conv: self.call_conv,
        }
    }

    /// Reads compiled functions back into the module, in the order given.
    ///
    /// Serial, and deliberately so: this writes to the module, and the order it
    /// writes in is the module's layout.
    fn define_all(&mut self, results: Vec<Emitted>) -> Result<(), TargetError> {
        for e in results {
            self.module
                .define_function_bytes(e.declared, e.alignment, &e.bytes, &e.relocs)?;
            self.faults.insert(e.id, e.faults);
            self.positions.insert(e.id, e.positions);
            self.entries.union(&e.entries);
            if let Some(entry) = self.emitted.get_mut(&e.id) {
                entry.1 = e.size;
            }
        }
        Ok(())
    }
}

/// What one function needs, all of it decided by the serial pre-pass.
///
/// Owns nothing that could hand out an identifier, which is what makes a batch
/// of these safe to move onto worker threads.
struct Planned<'a> {
    id: FuncId,
    declared: MachineFuncId,
    name: String,
    body: &'a Function,
    caches: Vec<DataId>,
}

/// Everything the parallel phase may read, and nothing it may write.
struct Shared<'a> {
    machine: &'a ModuleDeclarations,
    declarations: &'a Declarations,
    imports: &'a crate::symbols::EntryImports,
    heap: Option<&'a crate::mem::RegionBases>,
    heap_imports: Option<&'a crate::mem::HeapImports>,
    types: &'a crate::types::TypeRegistry,
    call_conv: CallConv,
}

/// Lowers one planned function.
///
/// A free function taking [`Shared`] rather than a method, so that the type
/// system says what the comment used to: this can run anywhere, because there is
/// nothing here to write to.
fn lower_one(shared: &Shared<'_>, planned: Planned<'_>) -> Result<Prepared, TargetError> {
    let lowered = crate::lower::lower_into(
        planned.body,
        shared.machine,
        shared.declarations,
        shared.imports,
        &planned.caches,
        shared.call_conv,
        shared.heap.map(|bases| crate::lower::Heap {
            bases,
            types: shared.types,
            // Present exactly when the heap has a symbolic base to read — see
            // `HeapImports`'s own doc for why there is at most one per heap.
            base: shared
                .heap_imports
                .and_then(|imports| imports.base())
                .map(|data| (shared.machine, data)),
        }),
    )?;

    let mut context = Context::new();
    context.func = lowered.func;
    Ok(Prepared {
        id: planned.id,
        declared: planned.declared,
        name: planned.name,
        entries: lowered.entries,
        context,
    })
}

/// How long `Context::compile` itself has taken, over this process.
///
/// Separated from what `compile_one` does around it — copying the code out,
/// reading the fault table and the position map — because those are OURS and
/// the code generator's own pass is not, and a single number for the pair says
/// nothing about which to work on.
static CODEGEN_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Machine-compiles one lowered function.
fn compile_one(isa: &dyn TargetIsa, mut p: Prepared) -> Result<Emitted, TargetError> {
    let started = std::time::Instant::now();
    p.context
        .compile(isa, &mut ControlPlane::default())
        .map_err(|error| TargetError::Compile(format!("in fn `{}`: {:?}", p.name, error.inner)))?;
    // A fresh, plain shared borrow — not the value `compile` returned, whose
    // lifetime is tied to `&mut p.context` and would make the `&p.context.func`
    // borrow below a borrow-checker error.
    // Cumulative for the process, not per batch: `compile_one` has no batch to
    // belong to — it is called from two places and runs on whatever worker took
    // it — and the question it answers is a ratio, which a total answers as well
    // as a delta would.
    CODEGEN_NANOS.fetch_add(
        started.elapsed().as_nanos() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    let code = p
        .context
        .compiled_code()
        .expect("compiled_code present after a successful compile");
    let relocs = code
        .buffer
        .relocs()
        .iter()
        .map(|r| ModuleReloc::from_mach_reloc(r, &p.context.func, p.declared))
        .collect();
    Ok(Emitted {
        id: p.id,
        declared: p.declared,
        alignment: code.buffer.alignment as u64,
        bytes: code.buffer.data().to_vec(),
        relocs,
        // Read out of `code` here rather than after `define_function_bytes`: the
        // correspondence between addresses and the program exists in what was
        // just compiled, and nowhere else afterwards.
        faults: crate::fault::FaultTable::of(code),
        positions: crate::observe::PositionMap::of(code),
        size: code.code_info().total_size as usize,
        entries: p.entries,
    })
}

/// A function lowered to the code generator's IR, waiting to be machine
/// compiled by [`MachineModule::compile_and_define`].
///
/// Owns its `Context` outright rather than borrowing anything from
/// `MachineModule` — nothing left in it needs `&mut dyn Module`, which is what
/// makes moving a batch of these onto worker threads sound.
pub struct Prepared {
    id: FuncId,
    declared: MachineFuncId,
    /// Only for the error a failed `Context::compile` names — a bail in a
    /// batch of hundreds is otherwise untraceable to which function it was.
    name: String,
    /// Which runtime entry points this body named, carried out of lowering
    /// because lowering has nowhere to record it any more — and recorded into
    /// the compilation serially, so the answer does not depend on scheduling.
    entries: crate::symbols::EntryTable,
    context: Context,
}

/// The machine code one [`Prepared`] function became.
///
/// Fully owned, so no borrow of the [`Context`] that produced it survives the
/// parallel phase — it is dropped with the closure that built this.
struct Emitted {
    id: FuncId,
    declared: MachineFuncId,
    alignment: u64,
    bytes: Vec<u8>,
    relocs: Vec<ModuleReloc>,
    faults: crate::fault::FaultTable,
    positions: crate::observe::PositionMap,
    size: usize,
    entries: crate::symbols::EntryTable,
}

/// Below this many functions in one batch, the serial path runs instead of the
/// rayon pool — in [`MachineModule::compile_all`] and in
/// [`MachineModule::compile_and_define`] alike.
///
/// A program of a handful of functions — a fixture, a REPL line, most unit
/// tests in this crate — is the common case while iterating, and building a
/// thread pool (and its 32 MiB-stacked workers, see [`pool`]) to compile a
/// handful of functions is pure loss with nothing to hide it behind. 8 is
/// `rts-codegen-new`'s `parcompile` measurement of where the equivalent split
/// starts paying for itself on this repository's hardware (its module doc has
/// the numbers); this crate had no such measurement of its own, so it borrowed
/// that one rather than inventing a different threshold with no evidence.
///
/// # Why it is 2 now, and what killed 8
///
/// A count is a bad proxy for the work in a batch, and this crate now has the
/// measurement it was missing. `tests/claude-math-complete.test.ts` compiles as
/// FOUR functions — one of them 12 979 IR instructions — and spends 56 ms of
/// CPU in the code generator. Under a threshold of 8 that batch took the serial
/// path, so its wall clock was the whole 56 ms; a 28-function file with a
/// comparable total (51 ms of CPU) took 8.7 ms of wall clock on the pool.
///
/// So the case the threshold was protecting — few functions — is exactly the
/// case that had the most to gain, because "few functions" in a compiled script
/// means one enormous one rather than a small program. Measured 2026-08-11,
/// `RTS_TIMING=1`, release.
///
/// It is not 1: a batch of one has nothing to overlap, so the pool could only
/// cost. Everything above that has at least two bodies to run at once, and the
/// pool is built once per process ([`pool`]'s `OnceLock`) rather than per batch.
const PARALLEL_THRESHOLD: usize = 2;

/// A dedicated rayon pool for the phase that lowers and machine-compiles.
///
/// `Context::compile` recurses over the function's IR and is stack-hungry — a
/// default rayon worker's stack is far smaller than the process main thread's.
/// `rts-codegen-new`'s `parcompile` hit exactly this: STATUS_STACK_OVERFLOW on
/// deep IR that compiled fine serially, fixed the same way here, before this
/// crate has a fixture deep enough to reproduce it — cheaper to carry the fix
/// than to wait for the crash that would justify it.
///
/// A pool of our own rather than rayon's global one, so this phase never
/// competes with another rayon user embedding this crate.
fn pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs())
            .stack_size(32 * 1024 * 1024)
            .thread_name(|i| format!("rts-cranelift-{i}"))
            .build()
            .expect("build the machine-compile thread pool")
    })
}

/// Worker count for the machine-compile pool. `RTS_CRANELIFT_JOBS` overrides
/// it — a bisect escape hatch, and the way to A/B this phase in one binary,
/// matching `RTS_CODEGEN_JOBS` in `rts-codegen-new`'s `parcompile`. `0` or
/// unset falls back to the host's parallelism.
fn jobs() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("RTS_CRANELIFT_JOBS")
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

impl MachineModule<'_> {
    /// Declares somewhere for each of a function's sites to remember what it saw.
    ///
    /// Writable, and initialized to a layout no object has. It cannot start at
    /// zero: zero is a real layout, and a site that had never run would claim to
    /// recognize the first one ever declared — and then read a field of an object
    /// of some other shape entirely, at an offset that happened to be there.
    ///
    /// Data rather than patched instructions, so that the same emission works
    /// whether the result runs from memory or is written to an object file. Code
    /// that rewrites itself cannot be written to a file at all.
    fn declare_caches(
        &mut self,
        id: FuncId,
        func: &Function,
    ) -> Result<Vec<cranelift_module::DataId>, TargetError> {
        let name = self
            .emitted
            .get(&id)
            .map(|(name, _)| name.clone())
            .ok_or(TargetError::UndeclaredFunction(id))?;

        // Which sites need the wider cell, derived from the function rather than
        // declared by a caller — rule 8, and the same derivation `record_shapes`
        // performs below for signatures. A caller that had to say would be a
        // caller that could say it for the wrong site, and the failure is a read
        // that loads two words past the end of its own cell.
        let indirect = Self::indirect_sites(func);

        let mut declared = Vec::with_capacity(func.cache_count());
        for site in 0..func.cache_count() {
            let data = self.module.declare_data(
                &format!("{name}.cache.{site}"),
                Linkage::Local,
                true,
                false,
            )?;
            // The layout no object has, in each width. It cannot start at zero:
            // zero is a real layout, and a site that had never run would claim
            // to recognize the first one ever declared. The wider form's third
            // word is zero because zero means "the cell asked about", which is
            // the answer a site that has never resolved must give — and its
            // fourth is -1 so that a cell claiming a second address it never
            // got could not also match a layout.
            let mut cold = Vec::with_capacity(32);
            cold.extend_from_slice(&(-1i64).to_ne_bytes());
            cold.extend_from_slice(&0i64.to_ne_bytes());
            if indirect.contains(&site) {
                cold.extend_from_slice(&0i64.to_ne_bytes());
                cold.extend_from_slice(&(-1i64).to_ne_bytes());
            }

            let mut description = cranelift_module::DataDescription::new();
            // Every load of these words is `MemFlags::trusted()`, which asserts
            // alignment; nothing asserted it before, which was a false claim
            // rather than a slow one. Rule 7 — an invariant is enforced.
            description.set_align(if indirect.contains(&site) { 32 } else { 16 });
            description.define(cold.into_boxed_slice());
            self.module.define_data(data, &description)?;
            declared.push(data);
        }
        Ok(declared)
    }

    /// Which of a function's sites read through a remembered address.
    ///
    /// Separate from the loop above so the answer is computed once for the
    /// function rather than rebuilt per site, and so the one fact that decides a
    /// cell's width has one place to be read from.
    fn indirect_sites(func: &Function) -> std::collections::BTreeSet<usize> {
        use crate::ir::Terminator;

        let mut wide = std::collections::BTreeSet::new();
        for (_, block) in func.blocks() {
            if let Some(Terminator::CachedGetIndirect { cache, .. }) = &block.terminator {
                wide.insert(cache.index());
            }
        }
        wide
    }

    /// Records every shape this function's indirect calls expect.
    ///
    /// An indirect call names a shape rather than a callee, and a shape has to
    /// exist in the module's vocabulary before a call site can refer to it.
    /// Deriving that from the function itself means a client never has to
    /// remember to declare a shape it already stated at the call site.
    fn record_shapes(&mut self, func: &Function, funcs: &FuncRegistry) {
        use crate::ir::{Inst, Terminator};

        let mut record = |sig: crate::ir::SigId| {
            if let Some(shape) = funcs.signature(sig) {
                let lowered = machine_signature(shape, self.call_conv);
                self.declarations.record_signature(sig, lowered);
            }
        };

        for (_, block) in func.blocks() {
            for &inst in &block.insts {
                if let Some(data) = func.inst(inst)
                    && let Inst::CallIndirect { sig, .. } = data.inst
                {
                    record(sig);
                }
            }
            if let Some(Terminator::TailCallIndirect { sig, .. }) = block.terminator {
                record(sig);
            }
        }
    }
}

/// What a compilation knows about its functions, once it is done with them.
///
/// Handed over rather than asked for while compiling, because placing a function
/// needs its address and nothing has one until the destination is finalized — by
/// which point the compilation is over. The borrow checker says the same thing,
/// which is how the ordering was noticed.
#[derive(Default)]
pub struct Placements {
    emitted: std::collections::HashMap<FuncId, (String, usize)>,
    positions: std::collections::HashMap<FuncId, crate::observe::PositionMap>,
}

impl Placements {
    /// Puts a compiled function on a map, at the address it ended up at.
    ///
    /// The address is the caller's to supply, because only the destination knows
    /// one. Everything else — the name, how much code it became, where each run
    /// of it came from — is already known, so none of it is restated. A length
    /// restated by hand is a length that can be wrong, and a wrong one names the
    /// wrong function for every address past the end.
    ///
    /// Reports whether there was anything to place. A function declared and never
    /// defined has a name and no code, and silently mapping zero bytes of it puts
    /// a hole in the map at a real address.
    pub fn place(&self, map: &mut crate::observe::CodeMap, id: FuncId, address: usize) -> bool {
        let Some((name, length)) = self.emitted.get(&id) else {
            return false;
        };
        if *length == 0 {
            return false;
        }

        let positions = self.positions.get(&id).cloned().unwrap_or_default();
        map.record(name, address, *length, positions);
        true
    }

    /// What a function was named and how much code it became.
    ///
    /// # Why this is observable at all
    ///
    /// Rule 13 says compiling one program twice produces the same thing, and
    /// since functions began compiling concurrently that is a property with
    /// something to break it. A property nothing can observe is a property
    /// nothing can test — so the size comes out, and `tests/determinism.rs`
    /// compares two compilations of one program through it.
    ///
    /// Not the bytes. They would be a stronger check and they are gone by the
    /// time this exists: the module owns them, and reaching back for them would
    /// be this structure holding a copy of the code to answer a question about
    /// its length. A size that differs is a different compilation, which is what
    /// the test needs to see.
    pub fn emitted(&self, id: FuncId) -> Option<(&str, usize)> {
        self.emitted
            .get(&id)
            .map(|(name, length)| (name.as_str(), *length))
    }

    /// Every function that has code to place.
    pub fn defined(&self) -> impl Iterator<Item = FuncId> + '_ {
        self.emitted
            .iter()
            .filter(|(_, (_, length))| *length > 0)
            .map(|(&id, _)| id)
    }
}

/// The architecture this process runs on.
///
/// Exposed because both destinations need one and neither should have to know
/// how to ask for it.
///
/// Addressed absolutely and tuned for [`Priority::CompileTime`], which is what
/// executable memory is: code compiled inside the wall clock of the program
/// about to run it. An object file asks [`isa_with`] for the other answer.
pub fn host_isa() -> Result<OwnedTargetIsa, TargetError> {
    isa_with(Addressing::Absolute, Priority::CompileTime)
}

/// Whether compiled code may name an address directly.
///
/// Not a preference: it is a question about where the code will END UP, and the
/// two destinations answer it differently. Code placed in this process's own
/// memory is at the address it was relocated to and stays there. Code written
/// to an object file is linked into an executable the loader may place anywhere.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Addressing {
    /// An address baked into the instruction stream.
    Absolute,
    /// Every address reached relative to the program counter.
    ///
    /// What a position-independent executable requires, and on some platforms
    /// the only thing they will load at all.
    Independent,
}

/// The architecture this process runs on, addressed the way a destination needs.
///
/// # Why the object file does not simply use [`host_isa`]
///
/// It did, and on macOS the result linked and then died with `Bus error: 10` the
/// moment it ran. Absolute addressing inside a position-independent executable
/// is a set of relocations the loader slides out from under the code: the
/// instruction holds the address the linker computed, the image is loaded
/// somewhere else, and the first access lands nowhere. Apple's arm64 target does
/// not offer the alternative — a PIE is the only thing the loader accepts — so
/// this is not a preference there either.
///
/// # Why every platform does not just get it
///
/// Because one of them was measured to break under it. `is_pic` on the COFF
/// target overflowed the stack during compilation (recorded when the AOT path
/// was first built), and Windows and ELF both link and run correctly today with
/// absolute addressing. Turning it on for them would be trading a working
/// destination for a symmetry nothing asked for; the platform that needs it gets
/// it, and this comment says which and why.
///
/// **This was not verified on a Mac.** No macOS machine was available; what is
/// verified is the failure it addresses — `rts compile bench/pi_machin.ts` then
/// `./smoke_aot` on the `macos-latest` runner, exit 138. The same job is what
/// confirms or refutes the fix.
/// Which side of the compile-time/code-quality trade a destination is on.
///
/// # Why this is a parameter and not a constant
///
/// Because the two destinations pay for compilation at different times, and the
/// code generator has one setting where that reverses the answer. Measured
/// 2026-08-11, release, `regalloc_algorithm`:
///
/// | | `backtracking` (default) | `single_pass` |
/// |---|---|---|
/// | placement, `claude-math-complete.test.ts` | 36.8 ms | 18.7 ms |
/// | a 3×10⁸-iteration loop, end to end | 5.32 s | 6.30 s |
/// | that file end to end | 52.9 ms | 43.0 ms |
///
/// A JIT run pays compilation inside the program's own wall clock, and almost
/// every program in this repository's corpus finishes before the better code
/// could earn its compilation back — the same file is 19% faster end to end.
/// An object file pays it once, at build time, and then runs for as long as
/// someone runs it, so the 18% it would lose in the loop is the only number
/// that matters there.
///
/// This is what rule 4 of this crate's README means by a difference that is
/// "about the destination": the PROGRAM is the same, and what differs is what
/// each destination is buying.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Priority {
    /// Compile as fast as possible; the code runs once, now. Executable memory.
    CompileTime,
    /// Generate the best code; compilation is paid once, elsewhere. Object file.
    CodeQuality,
}

/// The architecture this process runs on, addressed the way a destination needs
/// and tuned for what that destination is buying — see [`Priority`].
pub fn isa_with(
    addressing: Addressing,
    priority: Priority,
) -> Result<OwnedTargetIsa, TargetError> {
    let mut flags = cranelift_codegen::settings::builder();
    // Frame pointers are what makes a stack walkable, and a stack walk is how
    // the collector finds a frame with no descriptor. Turning them off would
    // save a register and cost the fallback that makes the migration possible.
    cranelift_codegen::settings::Configurable::set(&mut flags, "preserve_frame_pointers", "true")
        .expect("a real setting");
    // The code generator's own verifier, which is on by default and runs over
    // every function handed to it. It is left on in a debug build and turned off
    // in a release one, and `RTS_CL_VERIFY` turns it back on either way.
    //
    // # Why it is not simply left on
    //
    // Measured, 2026-08-11, release, `RTS_TIMING=1` on
    // `tests/claude-math-complete.test.ts`: 55 ms of placement with it, 45 ms
    // without — 19% of the phase that is itself 85% of compiling a file. That is
    // not a rounding error and it is paid on every run of every program.
    //
    // # Why turning it off is not a hole
    //
    // It checks the code generator's IR, which nothing but `lower/` writes, and
    // `verify/` has already rejected the representation that IR was produced
    // from. So it is a check of one translation, not of the program — and the
    // build where a mistake in that translation is made is a debug build, where
    // it stays on. Rule 12 says unproven behaviour fails safely, and this is the
    // shape of that: the conservative form is the default where a defect is
    // introduced, and the fast form is what a user runs.
    //
    // # What it does not say
    //
    // Nothing about `opt_level`, which was measured in the same session and
    // stayed at its default: `speed` moved placement from 55 ms to 70 ms and did
    // not move `run` on that file. A code-quality setting is a separate claim
    // needing a run long enough to show it, and this file is not one.
    // Which register allocator, which is the one setting where the two
    // destinations genuinely want opposite answers. `RTS_CL_REGALLOC` names one
    // explicitly, for the JIT program long-running enough to want the other.
    //
    // # `single_pass` is NOT the default any more, and this is why
    //
    // It was, for `CompileTime`, and it bought 25-49% off compiling a file. It
    // also **generated wrong machine code** for the largest program in this
    // workspace: the editor of the `rts-game` project segfaults 1.5 to 3 seconds
    // in, every run, with no message — and the same binary on the same scene
    // runs indefinitely with `backtracking`. Isolated with the flag this very
    // comment introduced:
    //
    // ```text
    // RTS_CL_REGALLOC=backtracking   exit 124, 124   (the timeout killed it)
    // RTS_CL_REGALLOC=single_pass    exit 139, 139   (SIGSEGV)
    // ```
    //
    // A minimal window program survives on `single_pass`, and every test in the
    // suite passes on it — which is exactly why this reached a default. The
    // corpus is 800 small files and the defect needs a big one, so "739 of 800,
    // LOST empty" was true and did not cover the case.
    //
    // The trade is not close. A compiler that finishes 25-49% sooner and emits
    // code that crashes has a negative value: the time it saves is spent by the
    // person who has to find out why, and this took an afternoon of bisection to
    // reach because the fault has no message and no backtrace.
    //
    // `RTS_CL_REGALLOC=single_pass` still names it, so the speed is available to
    // anyone who measures their own program and finds it survives. What changed
    // is which way the default fails: rule 12 says unproven behaviour fails
    // safely, and a segfault is the least safe failure there is.
    let algorithm = match std::env::var("RTS_CL_REGALLOC") {
        Ok(named) => named,
        // Both destinations take `backtracking` for now. The distinction the
        // priority draws is real and stays in the type; what does not stay is a
        // default that answers it with an allocator we have watched miscompile.
        Err(_) => match priority {
            Priority::CompileTime => "backtracking".to_owned(),
            Priority::CodeQuality => "backtracking".to_owned(),
        },
    };
    cranelift_codegen::settings::Configurable::set(&mut flags, "regalloc_algorithm", &algorithm)
        .expect("a real setting");
    // `opt_level`, which the paragraph above says "stayed at its default" —
    // and Cranelift's default is `none`, which gates out the WHOLE egraph
    // mid-end: no GVN, no LICM, no redundant-load elimination, and
    // `enable_alias_analysis` left `true` and inert
    // (`cranelift-codegen-0.131.0/src/context.rs:185`).
    //
    // That paragraph also says a code-quality claim needs "a run long enough to
    // show it" and that the test file it was measured on was not one. This knob
    // is what makes the longer run possible, and the answer is now measured
    // rather than open: on `bench/analytic.ts`, release, 2026-08-11, `speed`
    // returns NOISE — the empty-loop floor 7.2 → 7.0 ns, `prop read own`
    // 9.2 → 9.4 (worse), an array element read unmoved. The mid-end cannot
    // optimize across an opaque call, and this engine's IR is mostly opaque
    // calls, so there is little for it to see.
    //
    // Kept anyway, and env-gated so the default is bit-for-bit what it was,
    // because the claim has to be re-checkable: the day the runtime stops being
    // a call per operation is the day this setting starts mattering, and a
    // measurement nobody can repeat is a claim.
    //
    // NOT the same question for the AOT destination, which asks for
    // `Priority::CodeQuality` at `target/destination.rs` precisely to buy code
    // quality at build time and is currently buying it with the optimizer off.
    // No stated argument covers that, and this knob does not settle it.
    if let Ok(level) = std::env::var("RTS_CL_OPT") {
        cranelift_codegen::settings::Configurable::set(&mut flags, "opt_level", &level)
            .expect("a real setting");
    }
    let verify = match std::env::var_os("RTS_CL_VERIFY") {
        Some(_) => true,
        None => cfg!(debug_assertions),
    };
    if !verify {
        cranelift_codegen::settings::Configurable::set(&mut flags, "enable_verifier", "false")
            .expect("a real setting");
    }
    if addressing == Addressing::Independent {
        cranelift_codegen::settings::Configurable::set(&mut flags, "is_pic", "true")
            .expect("a real setting");
    }

    let flags = cranelift_codegen::settings::Flags::new(flags);
    let builder = cranelift_native::builder()
        .map_err(|message| TargetError::Module(ModuleError::Backend(anyhow_from(message))))?;
    builder
        .finish(flags)
        .map_err(|error| TargetError::Module(ModuleError::Backend(anyhow_from(error.to_string()))))
}

/// What an object file for THIS host has to be addressed as.
///
/// A fact about the platform's loader, so it is decided here rather than by
/// whoever asks for an object file — a caller that could choose would eventually
/// choose wrong on one platform and nowhere else, which is the failure that has
/// already happened once.
pub fn object_addressing() -> Addressing {
    match cfg!(target_vendor = "apple") {
        true => Addressing::Independent,
        false => Addressing::Absolute,
    }
}

/// Wraps a message the code generator gave us as an error it can carry.
fn anyhow_from(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::msg(message.into())
}
