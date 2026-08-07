//! Source text in, callable code out.

use rts_codegen::emit::{Ctx, emit_program};
use rts_codegen::names::Names;
use rts_codegen::parse::{parse_module, parse_script};
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
    /// The modules this program imported, in the order they must run — each an
    /// entry like [`Self::entry`], and empty for a single-file program.
    ///
    /// Run BEFORE the entry, because a module publishes its exports when its
    /// body finishes and the importer reads them when its own body starts.
    dependencies: Vec<Entry>,
    /// What the compiler decided the singletons are numbered.
    model: ValueModel,
    /// The heaps the code was built to address, one per thread it can run on.
    ///
    /// Held here because their bases are constants inside that code: a run
    /// supplying a different region would have every address point at memory
    /// nothing allocates in. One entry for a single-region program. Each is an
    /// `Option` only so a run can move it into a context and take it back.
    regions: Vec<Option<rts_core_rwk::heap::Region>>,
    /// Where those bases are listed, for a program compiled for several.
    ///
    /// `None` for one region, and the absence is the point: one region keeps its
    /// base as an immediate, so a single-threaded program never pays the two
    /// extra instructions per access `RegionBases::address_instructions` reports
    /// for the sharded form. Held and never read, like `placed`: the field IS
    /// the lifetime.
    #[allow(dead_code)]
    table: Option<rts_core_rwk::heap::BaseTable>,
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
    /// What each tagged-template site is made of, by literal position.
    ///
    /// Beside the literals because a site names them: the two are seeded
    /// together, into the same region, in that order.
    templates: Vec<Vec<u32>>,
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
    /// Every runtime entry point reaches for the thread's context — the heap,
    /// the shapes, the key registry — and a compiled program calls them without
    /// knowing that. So [`run_region`] installs one around the call and takes it
    /// back afterwards, which is what makes two runs independent.
    pub fn run(&mut self) -> u64 {
        // Region zero, moved in for the run and taken back after. Not copied:
        // there is one heap and its address is in the code.
        let region = self.regions[0]
            .take()
            .expect("the region is only absent while a run is in progress");
        let outcome = run_region(
            self.entry,
            &self.dependencies,
            self.model
                .singleton(rts_codegen::values::Singleton::Undefined)
                .word(),
            crate::link::singletons_for(&self.model),
            crate::link::kinds_for(&self.model),
            &self.keys,
            &self.literals,
            &self.templates,
            region,
        );
        self.regions[0] = Some(outcome.region);
        self.resolves = outcome.resolves;
        self.described = outcome.described;
        outcome.value
    }

    /// Runs the same program on `threads` threads, one region each.
    ///
    /// The `Context` is built **inside** each thread and never crosses one, so
    /// nothing about it has to be shareable — which is why this needs no `Send`
    /// bound and no lock. What crosses is a `Region`, two `Copy` numberings, two
    /// borrowed seed tables and the entry's address.
    ///
    /// A heap per thread and nothing more; see this crate's module
    /// documentation for what is absent, and do not read it as making a shared
    /// value safe.
    ///
    /// `resolves` and `described` are deliberately not updated: there are now N
    /// of each and one field, and picking one thread's would be a number that
    /// looks like the program's.
    ///
    /// # Panics
    ///
    /// When `threads` exceeds the count [`compile_for`] was given — that count
    /// is in every address computation, so a run cannot revisit it.
    pub fn run_on(&mut self, threads: usize) -> Vec<u64> {
        assert!(
            threads <= self.regions.len(),
            "compiled for {} regions, asked for {threads} threads: the selector \
             width is in the code and cannot be widened after placement",
            self.regions.len()
        );
        let taken: Vec<rts_core_rwk::heap::Region> = (0..threads)
            .map(|index| self.regions[index].take().expect("no run is in progress"))
            .collect();
        let singletons = crate::link::singletons_for(&self.model);
        let kinds = crate::link::kinds_for(&self.model);
        let nothing = self
            .model
            .singleton(rts_codegen::values::Singleton::Undefined)
            .word();
        let entry = self.entry;
        // Borrowed rather than cloned per thread, which is what scoped threads
        // are used here for: the alternative is N copies of every literal.
        let keys = &self.keys;
        let literals = &self.literals;
        let templates = &self.templates;

        let finished: Vec<(u64, rts_core_rwk::heap::Region)> = std::thread::scope(|scope| {
            let handles: Vec<_> = taken
                .into_iter()
                .map(|region| {
                    scope.spawn(move || {
                        let outcome =
                            run_region(
                                entry, &[], nothing, singletons, kinds, keys, literals,
                                templates, region,
                            );
                        (outcome.value, outcome.region)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a thread running compiled code"))
                .collect()
        });

        finished
            .into_iter()
            .enumerate()
            .map(|(index, (value, region))| {
                self.regions[index] = Some(region);
                value
            })
            .collect()
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

/// What one run left behind. The region comes back because it went in: a
/// `Context` owns it for the run, and the program outlives that context.
struct Outcome {
    value: u64,
    region: rts_core_rwk::heap::Region,
    resolves: u64,
    described: Option<String>,
}

/// Installs a context over one region and calls the entry.
///
/// A free function rather than a method, because [`Compiled::run_on`] calls it
/// from a thread that must not be able to name `self`: that nothing but scalars,
/// a region and two borrowed tables crosses is the whole property.
fn run_region(
    entry: Entry,
    // The modules the entry imports, run in order before it. See
    // `Compiled::dependencies`.
    dependencies: &[Entry],
    nothing: u64,
    singletons: rts_core_rwk::value::Singletons,
    kinds: rts_core_rwk::Kinds,
    keys: &[String],
    literals: &[String],
    templates: &[Vec<u32>],
    region: rts_core_rwk::heap::Region,
) -> Outcome {
    let mut context = rts_core_rwk::entry::Context::over(singletons, kinds, region);
    // The second agreement, alongside the singleton numbering: a property name
    // is resolved while compiling and crosses as a number, so this registry must
    // have issued that number or every property reads as absent. Seeded rather
    // than shared, because the compiler's is finished before this one exists.
    rts_core_rwk::entry::declare_keys(&mut context, keys);
    // The third, and the quietest if wrong: the code names a literal by its
    // position, so a table seeded from anything else makes every string the
    // wrong one. Seeded here rather than once — the values intern into THIS
    // region, and another region decodes them as absent.
    rts_core_rwk::entry::declare_literals(&mut context, literals);
    // After the literals, never before: a site names its pieces by position in
    // that table, so seeding these first would record numbers into a table about
    // to be cleared.
    rts_core_rwk::entry::declare_templates(&mut context, templates);
    // Before the context is installed, and every namespace here is built from
    // the `context` it is HANDED. A module reaching the ambient one instead
    // would be asking for a borrow this call already holds, which is a panic in
    // an `extern "C"` frame and therefore an abort.
    // What this crate can do and the module crates cannot: compile source. Handed
    // DOWN because they cannot reach up — this crate depends on them, so the
    // other direction is a cycle. See `entry::declare_evaluator`.
    rts_core_rwk::entry::declare_evaluator(&mut context, evaluate_source);
    rts_std_rwk::install(&mut context);
    rts_node_rwk::install(&mut context);
    // The modules a program may import. Registered by the HOST rather than by
    // the runtime, because which of them exist is a fact about the environment
    // the program is given — and `rts-std-rwk` is where anything needing an
    // operating system lives, which is the same availability rule that keeps
    // `Math` in the runtime and `io.print` out of it.
    let (context, (value, described)) = rts_core_rwk::entry::with_context(context, || {
        // A script closes over nothing, has no receiver and was passed no
        // arguments: `undefined` for all six, from the compiler's own numbering
        // rather than a constant written here.
        // Dependencies first, and their answers dropped: a module's value is
        // not what an importer reads — its published exports are, and it
        // publishes them as its body finishes.
        for dependency in dependencies {
            dependency(nothing, nothing, nothing, nothing, nothing, nothing);
        }
        let value = entry(nothing, nothing, nothing, nothing, nothing, nothing);
        // The turn ends here, not inside the program: a reaction must not run in
        // the entry point that queued it, and a rejection is only unhandled once
        // nothing more can attach to it.
        // The event loop, and it is the whole of one: drain what is already
        // queued, ask every registered source to deliver and to say when it
        // wants to be asked again, wait that long, repeat.
        //
        // This used to be two module names written here by hand —
        // `timers::drain()` and `worker_threads::join_all()` — and the four
        // other modules with background threads were simply not on the list.
        // Nothing pumped them, so an `fs.watch` started and then waited on
        // delivered nothing at all. A host that names its sources is a host that
        // forgets one; `entry::declare_loop_source` is what a module registers
        // itself with instead.
        //
        // Microtasks first and again inside: a reaction must not run in the
        // entry point that queued it, and a source may start work — a worker, a
        // timer — from inside one.
        loop {
            rts_core_rwk::entry::drain_microtasks();
            let Some(wait) = rts_core_rwk::entry::pump_sources() else {
                break;
            };
            // The waiting lives here rather than in the runtime, because
            // `std::thread::sleep` is not something every target has and
            // `rts-core-rwk`'s membership rule is availability. See
            // `entry::loops`.
            std::thread::sleep(wait);
        }
        rts_core_rwk::entry::drain_microtasks();
        // An uncaught throw, reported where this program ends.
        //
        // `rts_throw` used to print and `exit(1)` inline, which is what made a
        // `try` around a call uncompilable: a throw that ended the process could
        // never reach a handler one frame up. It records now, the machine returns
        // from the throwing function, and every call site asks — so the last
        // place a throw can still be in flight is here, with nobody left to ask.
        if let Some((tag, described)) = rts_core_rwk::entry::pending() {
            eprintln!("rts: uncaught exception (tag {tag}): {described}");
            // `exit`, not `abort`: this is a program ending because of something
            // the program did, and a core dump describes the engine rather than
            // the fault. Same choice the runtime used to make, in the one place
            // that can still make it.
            std::process::exit(1);
        }
        // Read while the context is still installed. A string's bytes are in the
        // slab beside its cell, so once the caller has the region back there is
        // nothing left to read it from.
        let described = rts_core_rwk::entry::described(value);
        (value, described)
    });
    Outcome {
        value,
        resolves: context.resolves,
        region: context.region,
        described,
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
    compile_for(source, 1)
}

/// The same, for a program that will run on up to `regions` threads at once.
///
/// The count is a parameter of COMPILATION because `regions` decides the
/// selector width, that decides what the low bits of every reference mean, and
/// every address computation in the emitted program masks and shifts by it. A
/// program placed for four regions cannot run on five, and running it on one
/// still costs the table load — so `compile` stays at one and this is the
/// opt-in, which is what keeps the single-threaded path on `Addressing::Single`.
///
/// # Panics
///
/// When `regions` is not a power of two. A selector is a mask.
pub fn compile_for(source: &str, regions: u32) -> Result<Compiled, HostError> {
    let mut names = Names::default();
    // A file that imports is compiled as a MODULE, and one that does not is
    // wrapped in a function as before. The distinction is not cosmetic: an
    // `import` is a syntax error inside a function body, so wrapping first and
    // asking later would refuse every module before anything could look at it —
    // which is what made the whole suite report zero.
    // `export` counts as much as `import`. It did not, and 25 files in the
    // corpus were parsed as scripts for it — where `export` is a syntax error,
    // so they were refused by the FRONT END with a message about module code
    // rather than by anything this compiler decided.
    //
    // A module whose parse FAILS reports its own error rather than falling
    // through. It used to `.ok()` and fall to the script path, where `import` is
    // a syntax error — so twenty files in the corpus were refused with a message
    // about module code when the real fault was something else entirely, and the
    // message named the wrapper this host wrote rather than anything in the
    // file. A diagnostic that points at the wrong thing is worse than a terse
    // one.
    let looks_like_a_module = source.contains("import ") || source.contains("export ");
    let module = match looks_like_a_module {
        true => Some(
            parse_module(source, &mut names)
                .map_err(|error| HostError::Parse(format!("{error:?}")))?,
        ),
        false => None,
    };
    // `async` when the source awaits at its top level. A script is wrapped in a
    // function, and `await` outside an async function is a SYNTAX error — so 14
    // files in the corpus were refused by the parser for a wrapper this host
    // wrote, not for anything they contained.
    let wrapper = match source.contains("await ") {
        true => "async function",
        false => "function",
    };
    // The newline before the closing brace is load-bearing. A file ending in a
    // `//` comment with no trailing newline put that brace INSIDE the comment,
    // so the wrapper never closed and the parser reported `Expected '}', got
    // '<eof>'` — twelve files in the corpus, refused for a character this host
    // wrote rather than for anything they contained.
    let wrapped = format!("{wrapper} {SCRIPT}() {{ {source}
 }}");
    // Parsed even when a module was: the script path needs it, and asking for it
    // here keeps ONE place where a parse failure becomes a `HostError`.
    let program = match &module {
        Some(_) => rts_codegen::syntax::Program::new(rts_codegen::syntax::Goal::Script),
        None => parse_script(&wrapped, &mut names)
            .map_err(|error| HostError::Parse(format!("{error:?}")))?,
    };

    let mut tags = TagRegistry::new();
    let model = ValueModel::declare(&mut tags);
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let mut calls = RuntimeCalls::new();
    let mut keys = KeyRegistry::new();

    // Unwrapping what was wrapped above. Anything other than the one function
    // declaration means the wrapping did not produce what it was written to
    // produce, which is a defect here rather than in the source.
    // Unwrapping what was wrapped above, for the script path only.
    let body: Vec<rts_codegen::syntax::Stmt> = match &module {
        Some(_) => Vec::new(),
        None => {
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
            body.clone()
        }
    };

    // Every function the program contains, not one. Emission declares each of
    // them itself now, including the script's own — a nested function has to be
    // declared before the body that defines it can take its address, so the
    // host declaring the entry afterwards stopped being possible.
    let emitted = {
        let mut ctx = Ctx::new(
            &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
        );
        let emitted = match &module {
            // A module binds its imports and then runs its statements — one
            // function, like a script, because what differs is what is in
            // scope before the first statement rather than how it is entered.
            Some(program) => rts_codegen::emit::emit_module(&program.body, &mut ctx),
            None => emit_program(&body, &mut ctx),
        };
        // The NAME, not its number. A `Name` is an index into a table this
        // function owns and the error crosses out of, so rendering it here is
        // the only place it can be done — and "a name nothing introduced"
        // without saying which is a diagnostic that cannot be acted on.
        match emitted {
            Err(rts_codegen::emit::EmitError::UnboundName(name)) => {
                return Err(HostError::Unbound(ctx.names.text(name).to_owned()));
            }
            other => other?,
        }
    };
    // The top-level body may `await`, and the verifier refuses that instruction
    // in a function whose signature does not say so. `emit_body` builds the
    // signature from `function::signature()`, which cannot know — only this
    // level knows whether the source it was handed awaits at its top level.
    //
    // Set here rather than threaded down, because the alternative is a parameter
    // on `emit_module` and `emit_program` that means "the host wrapped you in an
    // async function", which is a fact about this host rather than about either.
    let mut emitted = emitted;
    if source.contains("await ") {
        let entry = emitted.entry;
        if let Some((_, function)) = emitted.functions.iter_mut().find(|(id, _)| *id == entry) {
            function.signature.may_suspend = true;
        }
    }
    assemble(
        emitted,
        &[],
        regions,
        model,
        funcs,
        types,
        calls,
        names,
    )
}

/// Places a finished compilation and hands back something that can run it.
///
/// # Why this is shared between one file and a graph
///
/// Everything from here down is about MACHINE CODE — which symbols the program
/// asked for, where each function is placed, which heap its addresses are baked
/// against — and none of it changes when a program is several files instead of
/// one. The two callers differ only in what they emitted and which entries run
/// before the last, so that is all they pass.
#[allow(clippy::too_many_arguments)]
fn assemble(
    emitted: rts_codegen::emit::Program,
    dependency_ids: &[rts_cranelift::ir::FuncId],
    regions: u32,
    model: ValueModel,
    funcs: FuncRegistry,
    types: TypeRegistry,
    calls: RuntimeCalls,
    names: Names,
) -> Result<Compiled, HostError> {
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
    // From `RtEntry::ALL` and not from a list written here. It WAS such a list,
    // and it omitted the three promise operations — so the first compiled
    // `async function` reached finalization and died on "can't resolve symbol
    // rts_promise_new", with nothing between the machine emitting the call and
    // the JIT failing to find it. A hand-written list of everything is a list
    // that forgets one; the machine already enumerates them.
    for &entry in RtEntry::ALL {
        let address = machine_entry(entry);
        assert!(
            !address.is_null(),
            "the machine emits {} and this host has no address for it — a program \n             reaching it would die inside compiled code with no diagnostic",
            entry.symbol()
        );
        outside.push((entry.symbol(), address));
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
    // The heaps compiled code will address, made here rather than by a runtime
    // context, because a base — or the table of them — is a NUMBER baked into
    // the code. The context that runs the program must be the one holding these
    // regions, or every address it computes points at nothing.
    //
    // One region takes the SINGLE form deliberately, and that must not quietly
    // change: the sharded form costs a mask, a load and an add on every access,
    // and a program that never asked for a second thread must not pay them.
    const CELLS: u32 = 1 << 16;
    let (table, mut owned) = if regions <= 1 {
        (None, vec![rts_core_rwk::heap::Region::with_capacity(CELLS)])
    } else {
        // A precondition rather than a `HostError`: every variant of that enum
        // is about the SOURCE, and a region count is the embedder's own
        // argument. Reporting it as though the program were at fault would put
        // it where nobody looks.
        let several = rts_core_rwk::heap::Regions::new(regions, CELLS)
            .expect("the region count must be a power of two: a selector is a mask");
        let (table, list) = several.into_parts();
        (Some(table), list)
    };
    let bases = match &table {
        None => {
            let region = &owned[0];
            RegionBases::single(RegionBase::Immediate(region.base()), region.stride())
        }
        Some(table) => {
            let at = RegionBase::Immediate(table.address());
            RegionBases::sharded(at, regions, rts_core_rwk::heap::STRIDE)
                .expect("the machine refuses a count `Regions::new` just accepted")
        }
    };

    let placed = unsafe { place_in_memory(&placing, &outside, &funcs, &types, Some(bases))? };

    let address = placed
        .address_of(script)
        .expect("the script was placed with a body");
    // SAFETY: every compiled function is emitted under one convention, which
    // `Entry` spells and `emit_program` builds — including the script, which is
    // no longer a special shape.
    let entry: Entry = unsafe { std::mem::transmute(address) };
    // The same transmute for each module the entry imports, in the order the
    // loader put them: every one was placed with a body under the same
    // convention, so there is nothing special about the last.
    let dependencies: Vec<Entry> = dependency_ids
        .iter()
        .filter_map(|id| placed.address_of(*id))
        .map(|at| unsafe { std::mem::transmute::<*const u8, Entry>(at) })
        .collect();

    Ok(Compiled {
        placed,
        entry,
        dependencies,
        model,
        regions: owned.drain(..).map(Some).collect(),
        table,
        resolves: 0,
        described: None,
        literals: emitted.literals,
        templates: emitted.templates,
        keys: names.keyed_texts().into_iter().map(str::to_owned).collect(),
    })
}

/// Compiles and runs source text inside the program already running.
///
/// # Why the answer is a value and not a program
///
/// Because the caller is a native inside a running program — `node:vm`'s
/// `runInThisContext`, a `repl`'s line — and what it wants is what the source
/// produced, not something to place and enter later.
///
/// # What it shares with its caller, and what it does not
///
/// Nothing. `compile` builds a fresh program with its own key registry, literal
/// table and region, so source evaluated here cannot see the caller's variables
/// and the caller cannot see its declarations. That is `vm.runInNewContext`'s
/// semantics and NOT `eval`'s, which is why nothing here is called `eval`.
///
/// The region is the part that matters and the part that costs: a value the
/// evaluated program built lives in ITS region, and handing one back to a caller
/// addressing another region is a reference that means something else there. So
/// only a value needing no region crosses — a number, a boolean, a singleton —
/// and anything else answers `None` rather than a wrong object. Named rather
/// than discovered, and it is what a shared heap would remove.
fn evaluate_source(source: &str) -> Option<u64> {
    // An EXPRESSION answers itself, and that is what a caller of this asks for:
    // `vm.runInNewContext("1 + 2")` and a repl line both want the value, and
    // there is no completion value to give them — `compile` wraps a script in a
    // function, and a function that reaches its end answers `undefined`. So the
    // expression form is tried first and the plain one is the fallback, which is
    // what makes `let x = 1; x` still compile.
    //
    // Rejected: making the wrapper return its last expression statement for
    // every program. That changes what `compile` means for every caller,
    // including the suite, to fix something only this seam asks for.
    let expression = format!("return ({source});");
    let mut program = match compile(&expression) {
        Ok(program) => program,
        Err(_) => compile(source).ok()?,
    };
    let produced = program.run();
    // A reference belongs to the region that made it. Refusing to hand one over
    // is the whole of the safety here; a tagged non-reference is self-contained.
    match rts_core_rwk::value::Value(produced).as_slot() {
        Some(_) => None,
        None => Some(produced),
    }
}

/// Compiles a file and everything it imports, as one program.
///
/// # What this is that [`compile`] is not
///
/// A module system. `compile` takes source text and knows nothing about where it
/// came from, so `import { x } from "./other.ts"` answered `undefined` — the gap
/// `rts-core-rwk`'s `modules` doc names. This resolves the graph, reads every
/// file, and emits all of them into ONE compilation.
///
/// One compilation is not an optimisation; it is the only shape that works. A
/// module compiled separately would hold its exports in its own region, and a
/// reference belongs to the region that made it — so the importer could not
/// touch them. That is the wall `node:vm` and `worker_threads` met, and this
/// does not try to cross it.
///
/// # Order
///
/// Dependencies first, and each module publishes its exports as its body
/// finishes ([`rts_codegen::emit`]'s `emit_publications`). So by the time a
/// module's body starts, every namespace it imports from has been written.
pub fn compile_graph(entry: &std::path::Path) -> Result<Compiled, HostError> {
    let loaded = crate::graph::load(entry)?;
    let mut names = Names::default();

    // Parsed HERE, against the `Names` the whole compilation shares — the walk
    // that found the graph parsed each file too, with a table of its own, and
    // threw the trees away. A `Name` is an index, so a tree from that walk would
    // name locals by numbers this compilation never issued.
    let mut parsed = Vec::with_capacity(loaded.len());
    for file in &loaded {
        let mut program = parse_module(&file.source, &mut names)
            .map_err(|error| HostError::Parse(format!("{}: {error:?}", file.specifier)))?;
        // Every relative specifier becomes the path the loader resolved it to,
        // on BOTH sides: the import that reads and the re-export that forwards.
        // The runtime's table is a string comparison, and `./x` means different
        // files in two directories.
        crate::graph::rewrite(&mut program.body, &file.path);
        parsed.push(program);
    }

    let mut tags = TagRegistry::new();
    let model = ValueModel::declare(&mut tags);
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let mut calls = RuntimeCalls::new();
    let mut keys = KeyRegistry::new();

    let units: Vec<rts_codegen::emit::Unit<'_>> = loaded
        .iter()
        .zip(&parsed)
        .map(|(file, program)| rts_codegen::emit::Unit {
            specifier: file.specifier.clone(),
            items: &program.body,
        })
        .collect();

    let emitted = {
        let mut ctx = Ctx::new(
            &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
        );
        match rts_codegen::emit::emit_modules(&units, &mut ctx) {
            Err(rts_codegen::emit::EmitError::UnboundName(name)) => {
                return Err(HostError::Unbound(ctx.names.text(name).to_owned()));
            }
            other => other?,
        }
    };
    // The last unit is the entry — the file the caller named — and everything
    // before it is a dependency the loader ordered.
    let mut entries = emitted.entries;

    entries.pop();
    assemble(emitted.program, &entries, 1, model, funcs, types, calls, names)
}
