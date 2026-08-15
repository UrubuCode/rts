//! Source text in, callable code out.

use rts_codegen::emit::{Ctx, emit_program};
use rts_codegen::names::Names;
use rts_codegen::parse::{parse_module, parse_script};
use rts_codegen::runtime::{RuntimeCalls, RuntimeOp};
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

/// The tag a JavaScript throw carries, as this crate has to state it.
///
/// It is `rts_codegen::emit::protect::JS_THROW` and `rts_core::entry::throw`'s
/// `JS_THROW`, the third statement of one number — the shape `throw.rs` already
/// describes for the pair: neither crate may depend on the other in that
/// direction, so each states it and names the others.
///
/// This crate needs it because [`prepare`] asks the machine to rewrite every
/// generator body, and a resumption that unwinds has to leave with the tag the
/// program's own handlers were built for. `rts_cranelift` cannot choose it —
/// it compares tags and does not interpret them — and `rts_codegen` keeps its
/// copy private to the emitter, so the caller of the rewrite is where the two
/// meet, which is here.
const JS_THROW: rts_cranelift::unwind::Tag = rts_cranelift::unwind::Tag(1);

/// How the host enters compiled code.
///
/// The script is a JavaScript function like any other now, so it takes the
/// convention every compiled function takes: an environment, a receiver, and
/// four argument slots. It used to take nothing, which meant the host had a
/// second way into compiled code that only the entry used — and a second way in
/// is a second thing to keep in agreement with the callee.
pub(crate) type Entry = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

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
    regions: Vec<Option<rts_core::heap::Region>>,
    /// Where those bases are listed, for a program compiled for several.
    ///
    /// `None` for one region, and the absence is the point: one region keeps its
    /// base as an immediate, so a single-threaded program never pays the two
    /// extra instructions per access `RegionBases::address_instructions` reports
    /// for the sharded form. Held and never read, like `placed`: the field IS
    /// the lifetime.
    #[allow(dead_code)]
    table: Option<rts_core::heap::BaseTable>,
    /// How many cached read sites missed during the last run.
    ///
    /// Zero after a run whose sites all recognised what they saw. Equal to the
    /// number of reads when none of them did — which is what tells a cache that
    /// works from one that is a slower way of calling.
    resolves: u64,
    /// What a parked frame looks like, per generator body this program holds.
    ///
    /// Keyed by code address, which is fixed when the program is placed — so it
    /// is computed once, there, and seeded into every context that runs it.
    frames: Vec<rts_core::entry::FrameShape>,
    /// What each compiled function is called, by the address it was placed at.
    ///
    /// For a stack trace. Computed once, where the addresses become known, and
    /// seeded into every context that runs the program.
    function_names: Vec<(u64, String, u32)>,
    /// The text the last run's answer had, read while its heap still existed.
    described: Option<String>,
    /// Every string literal the compilation collected, as UTF-16 code units.
    ///
    /// Held rather than seeded once, because a context is built per run: the
    /// literals are interned into the heap, and a run that reused values from
    /// a previous context would name cells in a table that no longer exists.
    ///
    /// Units and not `String`, which is what `rts_core::entry::declare_literals`
    /// states: `"\uD83D"` is a legal string this table has to be able to hold.
    literals: Vec<Vec<u16>>,
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
        let _timing = rts_cranelift::probe::Phase::start("run");
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
            &self.frames,
            &self.function_names,
            region,
        );
        self.regions[0] = Some(outcome.region);
        self.resolves = outcome.resolves;
        // Beside the phase timings, because it is the same question asked of
        // the run rather than of the compile: a cached read that recognises
        // what it sees costs a load, and one that does not costs a call into
        // `cache_resolve`. A number near the read count means the caches are
        // missing every time, which is a 15x difference and looks like nothing
        // in a profile that only reports totals.
        if std::env::var_os("RTS_TIMING").is_some() {
            eprintln!("rts-timing cache misses  {:>8}", outcome.resolves);
        }
        if let Some(census) = &outcome.census {
            eprint!("{census}");
        }
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
        let taken: Vec<rts_core::heap::Region> = (0..threads)
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
        let frames = &self.frames;
        let function_names = &self.function_names;

        let finished: Vec<(u64, rts_core::heap::Region)> = std::thread::scope(|scope| {
            let handles: Vec<_> = taken
                .into_iter()
                .map(|region| {
                    scope.spawn(move || {
                        let outcome =
                            run_region(
                                entry, &[], nothing, singletons, kinds, keys, literals,
                                templates, frames, function_names, region,
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
    /// `rts_core::entry::described`. This exists because a caller cannot
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
    region: rts_core::heap::Region,
    resolves: u64,
    /// Every miss by reason, key and site, when `RTS_CACHE_CENSUS` asked.
    census: Option<String>,
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
    singletons: rts_core::value::Singletons,
    kinds: rts_core::Kinds,
    keys: &[String],
    literals: &[Vec<u16>],
    templates: &[Vec<u32>],
    // What a parked frame looks like, per generator body. Seeded like the
    // literals and for the same reason: the numbers were fixed when the program
    // was placed, and this context did not exist then.
    frames: &[rts_core::entry::FrameShape],
    // What each compiled function is called, for a stack trace to name a frame.
    function_names: &[(u64, String, u32)],
    region: rts_core::heap::Region,
) -> Outcome {
    let _seeding = rts_cranelift::probe::Phase::start("seed-context");
    let mut context = rts_core::entry::Context::over(singletons, kinds, region);
    // What lets `alloc`'s collection trigger scan this thread's stack at all —
    // see `crate::stack`'s own documentation for why the call lives here and
    // not in `rts-core`, and for what happens on a platform it cannot
    // answer for honestly.
    crate::stack::install(&mut context);
    // The second agreement, alongside the singleton numbering: a property name
    // is resolved while compiling and crosses as a number, so this registry must
    // have issued that number or every property reads as absent. Seeded rather
    // than shared, because the compiler's is finished before this one exists.
    rts_core::entry::declare_keys(&mut context, keys);
    // The third, and the quietest if wrong: the code names a literal by its
    // position, so a table seeded from anything else makes every string the
    // wrong one. Seeded here rather than once — the values intern into THIS
    // region, and another region decodes them as absent.
    rts_core::entry::declare_literals(&mut context, literals);
    // After the literals, never before: a site names its pieces by position in
    // that table, so seeding these first would record numbers into a table about
    // to be cleared.
    rts_core::entry::declare_templates(&mut context, templates);
    rts_core::entry::declare_frames(&mut context, frames.to_vec());
    rts_core::entry::declare_function_names(&mut context, function_names.to_vec());
    // Before the context is installed, and every namespace here is built from
    // the `context` it is HANDED. A module reaching the ambient one instead
    // would be asking for a borrow this call already holds, which is a panic in
    // an `extern "C"` frame and therefore an abort.
    // What this crate can do and the module crates cannot: compile source. Handed
    // DOWN because they cannot reach up — this crate depends on them, so the
    // other direction is a cycle. See `entry::declare_evaluator`.
    rts_core::entry::declare_evaluator(&mut context, evaluate_source);
    // The other half of that capability, and the half `evaluate_source` cannot
    // give: `new Function` needs a CALLABLE, which is a reference, and a
    // reference belongs to the region that made it. `crate::live` compiles into
    // THIS context's region instead of building one, which is why it is a
    // second injection rather than a second caller of the first.
    rts_core::entry::declare_function_compiler(&mut context, crate::live::compile_function);
    // The other capability that has to come down rather than up: letting time
    // pass. `rts-core`'s membership rule is availability and
    // `std::thread::sleep` is not on every target, so the runtime holds a hook
    // and this crate fills it — exactly as it does for compiling source.
    //
    // Nothing installed it, and the loop below hid that: the HOST slept between
    // turns, so a timer fired once the body had finished. What could not work
    // was `await` — `promise_await` asks `rest_for`, found no waiter, and
    // reported a promise only time could settle as a deadlock. So
    // `await new Promise(r => setTimeout(r, 5))`, the standard way to wait,
    // ended the program with "this promise cannot settle".
    rts_core::entry::declare_rest(&mut context, |wait| std::thread::sleep(wait));
    {
        let _timing = rts_cranelift::probe::Phase::start("install-std");
        rts_std::install(&mut context);
    }
    {
        let _timing = rts_cranelift::probe::Phase::start("install-node");
        rts_node::install(&mut context);
    }
    // Medido como os dois acima, e atras da mesma feature que o Cargo.toml
    // declara no default: `rts:rigid` e o solver de rigidos paralelo, e um
    // install que nao aparece na tabela de fases e um custo que ninguem ve.
    #[cfg(feature = "physics")]
    {
        let _timing = rts_cranelift::probe::Phase::start("install-physics");
        rts_physics::install(&mut context);
    }
    #[cfg(feature = "ui")]
    rts_ui::install(&mut context);
    // The modules a program may import. Registered by the HOST rather than by
    // the runtime, because which of them exist is a fact about the environment
    // the program is given — and `rts-std` is where anything needing an
    // operating system lives, which is the same availability rule that keeps
    // `Math` in the runtime and `io.print` out of it.
    drop(_seeding);
    let (context, (value, described)) = rts_core::entry::with_context(context, || {
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
            rts_core::entry::drain_microtasks();
            let Some(wait) = rts_core::entry::pump_sources() else {
                break;
            };
            // The waiting lives here rather than in the runtime, because
            // `std::thread::sleep` is not something every target has and
            // `rts-core`'s membership rule is availability. See
            // `entry::loops`.
            std::thread::sleep(wait);
        }
        rts_core::entry::drain_microtasks();
        // An uncaught throw, reported where this program ends.
        //
        // `rts_throw` used to print and `exit(1)` inline, which is what made a
        // `try` around a call uncompilable: a throw that ended the process could
        // never reach a handler one frame up. It records now, the machine returns
        // from the throwing function, and every call site asks — so the last
        // place a throw can still be in flight is here, with nobody left to ask.
        if let Some((tag, described)) = rts_core::entry::pending() {
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
        let described = rts_core::entry::described(value);
        (value, described)
    });
    // Depois do programa e antes de qualquer destrutor: um `wgpu::Device` solto
    // pelo destrutor de thread-local morre durante o descarregamento das DLLs do
    // driver. Ver `rts_ui::shutdown`. No-op quando nenhuma janela foi aberta.
    #[cfg(feature = "ui")]
    rts_ui::shutdown();
    // Rendered BEFORE the context is taken apart: the census names keys through
    // the interner, and the interner goes with the context.
    let census = rts_core::entry::census_report(&context);
    Outcome {
        value,
        resolves: context.resolves,
        census,
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
    let front = front_end(source)?;
    assemble(
        front.emitted,
        &[],
        regions,
        front.model,
        front.funcs,
        front.types,
        front.calls,
        front.names,
    )
}

/// Everything [`compile_for`] and [`crate::object::compile_to_object`] share:
/// source text parsed and emitted into one program, with nothing yet decided
/// about where it will be placed.
///
/// # Why this is its own function
///
/// Placement is the one thing that differs between a JIT run and an AOT object
/// — rule 4 of this crate's `README.md`, "both destinations, or neither" — and
/// everything above that line was, before this change, copy-pasted the moment a
/// second destination existed. A parser change or a new syntax form would then
/// have to be applied twice and would drift the moment it was not.
pub(crate) struct FrontEnd {
    pub emitted: rts_codegen::emit::Program,
    pub model: ValueModel,
    pub funcs: FuncRegistry,
    pub types: TypeRegistry,
    pub calls: RuntimeCalls,
    pub names: Names,
}

/// A numbering a compilation has to be made to AGREE with, because something is
/// already running against it.
///
/// # Why the direction reverses, and what `run.rs` says about the ordinary one
///
/// [`run_region`] seeds the runtime from the compilation — *"seeded rather than
/// shared, because the compiler's is finished before this one exists"*. That
/// sentence is a statement about ORDER, and it stops holding the moment a
/// second compilation happens while the first program is running: then the
/// runtime's numbering is the finished one, and the new compilation is what has
/// to line up with it.
///
/// What lining up costs is exactly this struct: the key texts and the literal
/// table are handed to `Ctx` before the first statement is emitted, so key `n`
/// and literal `n` mean here what they already mean there, and everything the
/// second compilation mints starts past the end of both.
pub(crate) struct Seed<'a> {
    /// The text of every property key already issued, in key order. `None`
    /// where the text is not expressible as Rust text — the position is still
    /// spent, or every later key would be numbered one lower here than there.
    pub keys: &'a [Option<String>],
    /// The literal table already seeded, in index order.
    pub literals: &'a [Vec<u16>],
}

/// Parses and emits source text into one program. See [`FrontEnd`].
pub(crate) fn front_end(source: &str) -> Result<FrontEnd, HostError> {
    front_end_agreeing(source, None)
}

/// The same, for a compilation that must agree with a numbering that already
/// exists. See [`Seed`].
pub(crate) fn front_end_agreeing(
    source: &str,
    seed: Option<&Seed<'_>>,
) -> Result<FrontEnd, HostError> {
    let _timing = rts_cranelift::probe::Phase::start("front-end");
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
    // The `#!` line goes FIRST, because the wrapper below would put it in the
    // middle of a program where it means nothing. `parse_as` strips it too, and
    // for a module that is the only place it happens — this is the script path,
    // whose source never reaches the parser unwrapped.
    let source = rts_codegen::parse::strip_shebang(source);
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
    // Before anything asks for a key, because minting is what fixes a number:
    // the first key this compilation asks for on its own has to land past the
    // last one the running program already issued.
    if let Some(seed) = seed {
        reserve_keys(&mut names, &mut keys, seed.keys);
    }

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
        let _timing = rts_cranelift::probe::Phase::start("emit");
        let mut ctx = Ctx::new(
            &model, &mut funcs, &mut calls, &mut keys, &mut names, &types,
        );
        // The same reservation for the literal table, and it has to happen here
        // rather than beside the keys: the table belongs to `Ctx`, and the only
        // way to occupy a position in it is to ask for the string that is
        // already there. Deduplicated by text, so a literal the new source
        // shares with the running program reuses the position it already has
        // instead of adding a second one.
        if let Some(seed) = seed {
            for units in seed.literals {
                ctx.literal_units(units);
            }
        }
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
    Ok(FrontEnd {
        emitted,
        model,
        funcs,
        types,
        calls,
        names,
    })
}

/// Spends the first `texts.len()` keys of a fresh registry on the names that
/// already have them, so that this compilation and the running runtime number
/// the same property the same way.
///
/// A placeholder for a name with no Rust spelling, because the POSITION is what
/// has to line up: skipping one would shift every key after it. `\0` cannot
/// begin an identifier or a property name a program writes, so the placeholder
/// can never be the name the source is asking about.
fn reserve_keys(names: &mut Names, keys: &mut KeyRegistry, texts: &[Option<String>]) {
    for (at, text) in texts.iter().enumerate() {
        let spelled = match text {
            Some(text) => text.clone(),
            None => format!("\u{0}rts-unnamed-key-{at}"),
        };
        let name = names.intern(&spelled);
        names.key(name, keys);
    }
}

/// Everything a placement needs that is not the destination: which names are
/// expected from the runtime, where each is, and what to define.
///
/// # Why this is shared rather than inline in [`assemble`]
///
/// Because a second caller places a program too — `crate::live`, for a `new
/// Function` body — and the set of symbols a program expects is one of the
/// three agreements this crate exists to hold. A second statement of it is a
/// second thing to keep in step with `rts-core`, which is the drift rule 2 of
/// this crate's `README.md` is about.
pub(crate) fn place(
    prepared: &Prepared,
    bases: rts_cranelift::mem::RegionBases,
) -> Result<InMemory, HostError> {
    let mut placing: Vec<Placing<'_>> = prepared
        .expected
        .iter()
        .map(|(op, id)| Placing {
            id: *id,
            name: op.symbol(),
            visibility: Visibility::Expected,
            body: None,
        })
        .collect();
    for ((id, body), name) in prepared
        .emitted
        .functions
        .iter()
        .zip(&prepared.names_for_placing)
    {
        placing.push(Placing {
            id: *id,
            // Every one is exported. Internal linkage would be right for the
            // ones only this program calls, and it is not what decides
            // anything here: the addresses are taken with `FuncAddr` inside
            // the same module either way.
            name,
            visibility: Visibility::Exported,
            body: Some(body),
        });
    }

    let mut outside: Vec<(&str, *const u8)> = prepared
        .expected
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

    // SAFETY: every address comes from `address_of`, which returns the runtime's
    // own entry points — functions in this binary, alive for the whole process,
    // whose signatures the runtime derives from the same Rust definitions the
    // compiler was told about.
    let _timing = rts_cranelift::probe::Phase::start("place");
    Ok(unsafe {
        place_in_memory(&placing, &outside, &prepared.funcs, &prepared.types, Some(bases))?
    })
}

/// The two tables that cannot exist until placement has chosen addresses: what
/// each function is called, and what each generator's parked frame looks like.
///
/// Shared with `crate::live` for the reason [`place`] is: a run-time
/// compilation seeds the same two tables, and computing an address-keyed table
/// twice is how the two come to disagree about which body a shape describes.
pub(crate) fn addressed(
    prepared: &Prepared,
    placed: &InMemory,
) -> (Vec<(u64, String, u32)>, Vec<rts_core::entry::FrameShape>) {
    let function_names = prepared
        .emitted
        .function_names
        .iter()
        .filter_map(|(id, name, arity)| {
            let at = placed.address_of(*id)?;
            Some((at as u64, name.clone(), *arity))
        })
        .collect();
    // A body that was rewritten and then not placed would leave a wrapper
    // handing over an address nothing describes, which the runtime answers
    // `undefined` for rather than guessing.
    let frames = prepared
        .frames
        .iter()
        .filter_map(|(id, shape)| {
            let at = placed.address_of(*id)?;
            Some(rts_core::entry::FrameShape {
                code: at as u64,
                ..shape.clone()
            })
        })
        .collect();
    (function_names, frames)
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
/// What emission produced, rewritten and checked into the shape either
/// destination places — the part of `assemble` that is not about MACHINE CODE
/// and so must not become two copies the day an object path exists.
///
/// # What is deliberately absent
///
/// [`Prepared::frames`] carries a `code: 0` for every entry, same as the old
/// single-function `assemble` did — an address is not a number anybody holds
/// until placement decides it, and placement is the one thing that differs
/// between the two destinations.
pub(crate) struct Prepared {
    pub emitted: rts_codegen::emit::Program,
    pub funcs: FuncRegistry,
    pub types: TypeRegistry,
    pub script: rts_cranelift::ir::FuncId,
    pub frames: Vec<(rts_cranelift::ir::FuncId, rts_core::entry::FrameShape)>,
    pub expected: Vec<(RuntimeOp, rts_cranelift::ir::FuncId)>,
    pub names_for_placing: Vec<String>,
}

/// Rewrites every generator body, collects the runtime operations this program
/// actually calls (checked against what the runtime defines), names every
/// function for placement, and verifies every body — all of it destination
/// agnostic. See [`Prepared`].
pub(crate) fn prepare(
    emitted: rts_codegen::emit::Program,
    funcs: FuncRegistry,
    types: TypeRegistry,
    calls: RuntimeCalls,
) -> Result<Prepared, HostError> {
    let _timing = rts_cranelift::probe::Phase::start("prepare");
    let script = emitted.entry;
    let mut emitted = emitted;
    let mut funcs = funcs;
    let mut types = types;

    // Every generator body, rewritten into the form that can be parked and
    // picked up again. This is the host's half of `docs/engine/generators.md`
    // and it happens HERE, between emission and placement, for one reason: the
    // frame is an aggregate that does not exist until the rewrite runs, and this
    // is the layer that holds the type registry it is declared in.
    //
    // The identifier is kept and its shape corrected. A second identifier for
    // the rewritten form would leave the wrapper's `FuncAddr` pointing at the
    // body that still contains the suspension — a function nothing can enter.
    let mut frames: Vec<(rts_cranelift::ir::FuncId, rts_core::entry::FrameShape)> = Vec::new();
    for id in std::mem::take(&mut emitted.generators) {
        let Some((_, body)) = emitted.functions.iter_mut().find(|(this, _)| *this == id) else {
            continue;
        };
        let resumable = rts_cranelift::frame::resumable_form(body, &mut types, JS_THROW)
            .map_err(|error| HostError::Malformed(format!("{error:?}")))?;
        funcs
            .redeclare(id, resumable.func.signature.clone())
            .ok_or_else(|| HostError::Malformed("a generator body nothing declared".to_owned()))?;
        let layout = rts_cranelift::mem::ObjectLayout::of(resumable.layout.ty, &types);
        frames.push((
            id,
            rts_core::entry::FrameShape {
                code: 0,
                ty: resumable.layout.ty.index() as u32,
                size: layout.size,
                slots: layout.field_offsets.len() as u32,
                label_field: resumable.layout.label_field,
                resumed_field: resumable.layout.resumed_field,
                mode_field: resumable.layout.mode_field,
                param_fields: resumable.layout.param_fields.clone(),
                return_field: resumable.layout.return_fields.first().copied(),
            },
        ));
        *body = resumable.func;
    }

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

    // A name per function, because placement addresses by one. Only the script
    // needs a *meaningful* name — it is what the host looks up afterwards —
    // and the rest are numbered, because a JavaScript function need not have a
    // name and two that do may share it.
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

    // Asked before anything is placed, and it earns its line immediately: the
    // first program that returned `1 === 1` handed back a machine boolean where
    // its signature declared a tagged value, and went straight to the code
    // generator because nothing here had asked.
    //
    // Every function, not just the entry. A nested one is exactly as able to
    // be malformed, and it is the one a reader is least likely to look at.
    for (_, body) in &emitted.functions {
        let complaints = rts_cranelift::verify::verify(body, &types, &funcs);
        if !complaints.is_empty() {
            return Err(HostError::Malformed(format!("{complaints:?}")));
        }
    }

    Ok(Prepared {
        emitted,
        funcs,
        types,
        script,
        frames,
        expected,
        names_for_placing,
    })
}

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
    let prepared = prepare(emitted, funcs, types, calls)?;

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
        (None, vec![rts_core::heap::Region::with_capacity(CELLS)])
    } else {
        // A precondition rather than a `HostError`: every variant of that enum
        // is about the SOURCE, and a region count is the embedder's own
        // argument. Reporting it as though the program were at fault would put
        // it where nobody looks.
        let several = rts_core::heap::Regions::new(regions, CELLS)
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
            RegionBases::sharded(at, regions, rts_core::heap::STRIDE)
                .expect("the machine refuses a count `Regions::new` just accepted")
        }
    };

    let placed = place(&prepared, bases)?;

    // Now the addresses exist, so the two address-keyed tables can be built:
    // what each function is called, for a stack trace, and what each generator's
    // parked frame looks like.
    let (function_names, frames) = addressed(&prepared, &placed);

    let address = placed
        .address_of(prepared.script)
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
        frames,
        function_names,
        described: None,
        literals: prepared.emitted.literals,
        templates: prepared.emitted.templates,
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
    match rts_core::value::Value(produced).as_slot() {
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
/// `rts-core`'s `modules` doc names. This resolves the graph, reads every
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
    let (front, entries) = crate::graph::front_end(entry)?;
    assemble(
        front.emitted,
        &entries,
        1,
        front.model,
        front.funcs,
        front.types,
        front.calls,
        front.names,
    )
}
