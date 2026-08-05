//! Source text in, callable code out.

use rts_codegen::emit::{Ctx, emit_program};
use rts_codegen::names::Names;
use rts_codegen::parse::parse_script;
use rts_codegen::runtime::RuntimeCalls;
use rts_codegen::syntax::{FunctionBody, ModuleItem, StmtKind};
use rts_codegen::values::ValueModel;
use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::mem::{RegionBase, RegionBases};
use rts_cranelift::shape::KeyRegistry;
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::tags::TagRegistry;
use rts_cranelift::target::{InMemory, Placing, Visibility, place_in_memory};
use rts_cranelift::types::TypeRegistry;

use crate::entries::{agree, machine_entry, resolve};
use crate::link::HostError;

/// The name a compiled script is placed under.
///
/// Not derived from anything in the source: a script has no name, and inventing
/// one from a file path would make the symbol depend on where the file was.
const SCRIPT: &str = "__rts_script";

/// How the host enters compiled code.
///
/// The script is a JavaScript function like any other now, so it takes the
/// convention every compiled function takes: an environment, a receiver, and
/// four argument slots. It used to take nothing, which meant the host had a
/// second way into compiled code that only the entry used — and a second way in
/// is a second thing to keep in agreement with the callee.
type Entry = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// A compiled program, and the memory its code lives in.
///
/// `Debug` reports what it is rather than what it holds: an address and a JIT
/// module say nothing a reader can act on, and a test that prints one wants to
/// know it got a program at all.
pub struct Compiled {
    /// Keeps the pages alive. Dropping this invalidates [`Self::entry`], which
    /// is why it is held and never read: the field IS the lifetime, and a
    /// version without it compiles, runs once, and calls freed memory the
    /// second time.
    #[allow(dead_code)]
    placed: InMemory,
    entry: Entry,
    /// What the compiler decided the singletons are numbered.
    model: ValueModel,
    /// The heap the compiled code was built to address.
    ///
    /// Held here because its base is a constant inside that code: the program
    /// and the region are one thing, and a run that supplied a different region
    /// would have every address point at memory nothing allocates in.
    ///
    /// An `Option` only so a run can move it into the context and take it back —
    /// it is never absent between runs.
    region: Option<rts_core_rwk::heap::Region>,
    /// How many cached read sites missed during the last run.
    ///
    /// Zero after a run whose sites all recognised what they saw. Equal to the
    /// number of reads when none of them did — which is what tells a cache that
    /// works from one that is a slower way of calling.
    resolves: u64,
    /// The text the last run's answer had, read while its heap still existed.
    described: Option<String>,
    /// The text of every string literal the compilation collected.
    ///
    /// Held rather than seeded once, because a context is built per run: the
    /// literals are interned into the heap, and a run that reused values from
    /// a previous context would name cells in a table that no longer exists.
    literals: Vec<String>,
    /// The text of every property key the compilation minted, in key order.
    ///
    /// This was a COUNT, and a count was enough while every key was one the
    /// compiler had resolved: both sides hold the same number and neither needs
    /// the text. A computed key `o[k]` arrives at the runtime as a string and
    /// has to reach the number the compiler already chose, which a count cannot
    /// say — so the texts cross, in the order that mints those numbers.
    keys: Vec<String>,
}

impl std::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Compiled({} placed)", self.placed.len())
    }
}

impl Compiled {
    /// Runs it, and hands back the encoded value it produced.
    ///
    /// # Why a context is installed here
    ///
    /// Every runtime entry point reaches for the thread's context — the heap,
    /// the shapes, the key registry — and a compiled program calls them without
    /// knowing that. So the host installs one around the call and takes it back
    /// afterwards, which is also what makes two runs independent.
    pub fn run(&mut self) -> u64 {
        let singletons = crate::link::singletons_for(&self.model);
        // The region the program was compiled against, moved in for the run and
        // taken back after. Not copied: there is one heap, and its address is in
        // the code.
        let region = self
            .region
            .take()
            .expect("the region is only absent while a run is in progress");
        let mut context = rts_core_rwk::entry::Context::over(singletons, region);
        // The second agreement, alongside the singleton numbering. A property
        // name is resolved while compiling and crosses as a number, so the
        // runtime's registry must have issued that number — otherwise it
        // refuses it and every property reads as absent.
        //
        // Seeded rather than shared, because the two registries live in
        // different phases: the compiler's is finished before the runtime's
        // exists. Issuing the same count is what makes them the same registry
        // for every purpose that matters.
        rts_core_rwk::entry::declare_keys(&mut context, &self.keys);
        // The third agreement, and the one whose absence is quietest: the code
        // names a literal by its position, so a table seeded from anything but
        // what this compilation collected makes every string the wrong one —
        // or, past the end, absent. Seeded per run because the values are
        // interned into this run's heap.
        rts_core_rwk::entry::declare_literals(&mut context, &self.literals);
        let entry = self.entry;
        // A script closes over nothing, has no receiver, and was passed no
        // arguments. `undefined` for all six, from the compiler.s own numbering
        // rather than from a constant written here.
        let nothing = self
            .model
            .singleton(rts_codegen::values::Singleton::Undefined)
            .word();
        let (context, (value, text)) = rts_core_rwk::entry::with_context(context, || {
            let value = entry(nothing, nothing, nothing, nothing, nothing, nothing);
            // The turn ends here, not inside the program: a reaction must not
            // run in the entry point that queued it, and a rejection is only
            // unhandled once nothing more can attach to it.
            rts_core_rwk::entry::drain_microtasks();
            // Read while the context is still installed. A string is a cell in
            // the region with its bytes beside it in the slab, so once this
            // function has taken the region back there is nothing left to read
            // it from — and a caller holding only a word cannot ask later.
            let text = rts_core_rwk::entry::described(value);
            (value, text)
        });
        self.region = Some(context.region);
        self.resolves = context.resolves;
        self.described = text;
        value
    }

    /// The text the last run produced, when its answer had any.
    ///
    /// `None` for an object, whose conversion runs user code — see
    /// `rts_core_rwk::entry::described`. This exists because a caller cannot
    /// read a string out of a word after the run: the heap it names is gone.
    pub fn described(&self) -> Option<&str> {
        self.described.as_deref()
    }

    /// What the compiler numbered the singletons, for a caller reading a result.
    pub fn model(&self) -> &ValueModel {
        &self.model
    }

    /// How many cached reads missed during the last run.
    pub fn resolves(&self) -> u64 {
        self.resolves
    }
}

/// Compiles a function body into this process's memory.
///
/// # Why a body and not a script
///
/// A program has to be able to say what it produced, and in JavaScript the way
/// a body says that is `return` — which is a **syntax error** at the top level
/// of a script. The first version of this took a script and every test that
/// returned anything failed to parse.
///
/// A script does have an answer to "what did it produce": its *completion
/// value*, which is what `eval` hands back, and it is a real part of the
/// specification rather than a convenience. It is also a piece of work that has
/// not been done — the completion value is not simply the last statement, since
/// an empty block and a `var` declaration produce nothing while an `if` produces
/// its taken branch's value.
///
/// So the source is wrapped in a function and its body compiled, which is
/// exactly what the language says a `return` belongs to. When the completion
/// value is implemented, this gains a second entry point rather than changing
/// what this one means.
pub fn compile(source: &str) -> Result<Compiled, HostError> {
    let mut names = Names::default();
    let wrapped = format!("function {SCRIPT}() {{ {source} }}");
    let program = parse_script(&wrapped, &mut names)
        .map_err(|error| HostError::Parse(format!("{error:?}")))?;

    let mut tags = TagRegistry::new();
    let model = ValueModel::declare(&mut tags);
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let mut calls = RuntimeCalls::new();
    let mut keys = KeyRegistry::new();

    // Unwrapping what was wrapped above. Anything other than the one function
    // declaration means the wrapping did not produce what it was written to
    // produce, which is a defect here rather than in the source.
    let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
        return Err(HostError::Parse(
            "the wrapper did not produce one statement".to_owned(),
        ));
    };
    let StmtKind::Function(function) = &statement.kind else {
        return Err(HostError::Parse(
            "the wrapper did not produce a function".to_owned(),
        ));
    };
    let FunctionBody::Block(body) = &function.body else {
        return Err(HostError::Parse(
            "a declaration always has a block body".to_owned(),
        ));
    };

    // Every function the program contains, not one. Emission declares each of
    // them itself now, including the script's own — a nested function has to be
    // declared before the body that defines it can take its address, so the
    // host declaring the entry afterwards stopped being possible.
    let emitted = {
        let mut ctx = Ctx::new(
            &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
        );
        emit_program(body, &mut ctx)?
    };
    let script = emitted.entry;

    // Every runtime operation the program actually asked for, and only those.
    // A compilation that never concatenates carries no reference to the string
    // path, which is what `RuntimeCalls` declaring on demand is for.
    let mut expected = Vec::new();
    for (op, id) in calls.declared() {
        // Checked before anything is placed, and only for what this program
        // actually asked for — an operation nothing called cannot skew a call
        // that does not exist. The whole set is checked by a test instead,
        // because the failure worth catching is in the operation nobody has
        // exercised yet.
        agree(op)?;
        expected.push((op, id));
    }

    let mut placing: Vec<Placing<'_>> = expected
        .iter()
        .map(|(op, id)| Placing {
            id: *id,
            name: op.symbol(),
            visibility: Visibility::Expected,
            body: None,
        })
        .collect();
    // A name per function, because placement addresses by one. Only the script
    // needs a *meaningful* name — it is what the host looks up afterwards —
    // and the rest are numbered, because a JavaScript function need not have a
    // name and two that do may share it. The id is what a call site holds;
    // these strings exist for the placement surface and for a backtrace.
    //
    // Held in their own vector because `Placing` borrows them, and a name built
    // inside the loop that fills `placing` would not outlive it.
    let names_for_placing: Vec<String> = emitted
        .functions
        .iter()
        .map(|(id, _)| {
            if *id == script {
                SCRIPT.to_owned()
            } else {
                format!("__rts_fn_{}", id.index())
            }
        })
        .collect();
    for ((id, body), name) in emitted.functions.iter().zip(&names_for_placing) {
        placing.push(Placing {
            id: *id,
            name,
            // Every one is exported. Internal linkage would be right for the
            // ones only this program calls, and it is not what decides
            // anything here: the addresses are taken with `FuncAddr` inside
            // the same module either way.
            visibility: Visibility::Exported,
            body: Some(body),
        });
    }

    let mut outside: Vec<(&str, *const u8)> = expected
        .iter()
        .map(|(op, _)| (op.symbol(), resolve(*op).1))
        .collect();

    // The machine's own entry points, which it dials without being asked: a
    // program that allocates calls `rts_alloc`, and a cached read that misses
    // calls `rts_cache_resolve`. Neither is a `RuntimeOp` — the language never
    // names them — so neither appears in what the compilation declared, and
    // supplying them is the host's job rather than something to discover from a
    // missing symbol.
    //
    // Given unconditionally rather than when a program looks like it needs one:
    // a JIT resolves a name at finalization and an unused address costs a row
    // in a table, while a missing one is a crash with no diagnostic.
    for entry in [RtEntry::Alloc, RtEntry::CacheResolve, RtEntry::WriteBarrier] {
        outside.push((entry.symbol(), machine_entry(entry)));
    }

    // Asked before anything is placed, and it earns its line immediately: the
    // first program that returned `1 === 1` handed back a machine boolean where
    // its signature declared a tagged value, and went straight to the code
    // generator because nothing here had asked.
    //
    // The verifier had the check all along. Skipping it made the machine's
    // answer to "is this well formed" unavailable at the one moment it was
    // about to matter.
    // Every function, not just the entry. A nested one is exactly as able to
    // be malformed, and it is the one a reader is least likely to look at.
    for (_, body) in &emitted.functions {
        let complaints = rts_cranelift::verify::verify(body, &types, &funcs);
        if !complaints.is_empty() {
            return Err(HostError::Malformed(format!("{complaints:?}")));
        }
    }

    // SAFETY: every address comes from `address_of`, which returns the runtime's
    // own entry points — functions in this binary, alive for the whole process,
    // whose signatures the runtime derives from the same Rust definitions the
    // compiler was told about.
    // The heap compiled code will address. Its base is this region's, so the
    // region has to outlive the program — which is why the compiled program
    // owns it rather than the runtime context creating its own.
    //
    // A second consequence, and the reason this is wired here rather than in the
    // runtime: the base is a NUMBER baked into the compiled code. The context
    // that runs the program must be the one holding this region, or every
    // address the program computes points into a region that no longer exists.
    let region = rts_core_rwk::heap::Region::with_capacity(1 << 16);
    let bases = RegionBases::single(RegionBase::Immediate(region.base()), region.stride());
    let stride = region.stride();
    let _ = stride;

    let placed = unsafe { place_in_memory(&placing, &outside, &funcs, &types, Some(bases))? };

    let address = placed
        .address_of(script)
        .expect("the script was placed with a body");
    // SAFETY: every compiled function is emitted under one convention, which
    // `Entry` spells and `emit_program` builds — including the script, which is
    // no longer a special shape.
    let entry: Entry = unsafe { std::mem::transmute(address) };

    Ok(Compiled {
        placed,
        entry,
        model,
        region: Some(region),
        resolves: 0,
        described: None,
        literals: emitted.literals,
        keys: names.keyed_texts().into_iter().map(str::to_owned).collect(),
    })
}
