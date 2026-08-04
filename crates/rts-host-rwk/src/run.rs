//! Source text in, callable code out.

use rts_codegen::emit::{Ctx, emit_program};
use rts_codegen::names::Names;
use rts_codegen::parse::parse_script;
use rts_codegen::runtime::{RuntimeCalls, RuntimeOp};
use rts_codegen::syntax::{FunctionBody, ModuleItem, StmtKind};
use rts_codegen::values::ValueModel;
use rts_core_rwk::entry::CoreEntry;
use rts_cranelift::abi::AbiType;
use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::repr::Repr;
use rts_cranelift::mem::{RegionBase, RegionBases};
use rts_cranelift::shape::KeyRegistry;
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::tags::TagRegistry;
use rts_cranelift::target::{InMemory, Placing, Visibility, place_in_memory};
use rts_cranelift::types::TypeRegistry;

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
    /// The text of every string literal the compilation collected.
    ///
    /// Held rather than seeded once, because a context is built per run: the
    /// literals are interned into the heap, and a run that reused values from
    /// a previous context would name cells in a table that no longer exists.
    literals: Vec<String>,
    /// How many property keys the compilation minted.
    ///
    /// The runtime has to have issued the same ones, or a number the program
    /// carries names nothing. Kept as a count rather than as the registry
    /// itself: a registry issues in order, so the count IS the agreement, and
    /// two registries that issued the same number of keys agree about every one
    /// of them.
    keys: usize,
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
        if self.keys > 0 {
            context.keys.declare(self.keys as u32);
        }
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
        let (context, value) = rts_core_rwk::entry::with_context(context, || {
            entry(nothing, nothing, nothing, nothing, nothing, nothing)
        });
        self.region = Some(context.region);
        self.resolves = context.resolves;
        value
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
        let mut ctx = Ctx::new(&model, &mut funcs, &mut calls, &mut keys, &mut names, &types);
        emit_program(body, &mut ctx)?
    };
    let script = emitted.entry;

    // Every runtime operation the program actually asked for, and only those.
    // A compilation that never concatenates carries no reference to the string
    // path, which is what `RuntimeCalls` declaring on demand is for.
    let mut expected = Vec::new();
    for (op, id) in calls.declared() {
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
        .map(|(op, _)| Ok((op.symbol(), address_of(*op)?)))
        .collect::<Result<_, HostError>>()?;

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
        literals: emitted.literals,
        keys: keys.len(),
    })
}

/// The runtime's implementation of one operation.
///
/// # Why a match and not a table
///
/// Because the arm that does not exist is the point. `RuntimeOp` is stated in
/// `rts-codegen` and the implementations are exported by `rts-core-rwk`, and the
/// two are written independently — so this is where a disagreement between them
/// becomes a refusal instead of a call to whatever the linker happened to find.
///
/// Adding an operation to the compiler without adding it to the runtime fails
/// here, by name, at the moment a program first needs it.
fn address_of(op: RuntimeOp) -> Result<*const u8, HostError> {
    // Each cast names the ABI shape the compiled program was built to expect.
    // Writing it out is what makes a signature change on either side a type
    // error here rather than a corrupt call.
    Ok(match op {
        RuntimeOp::Add => rts_core_rwk::entry::add as extern "C" fn(u64, u64) -> u64 as *const u8,
        RuntimeOp::StrictEquals => {
            rts_core_rwk::entry::strict_equals as extern "C" fn(u64, u64) -> bool as *const u8
        }
        RuntimeOp::ToBoolean => {
            rts_core_rwk::entry::to_boolean as extern "C" fn(u64) -> bool as *const u8
        }
        RuntimeOp::NumberToString => {
            rts_core_rwk::entry::number_to_string as extern "C" fn(f64) -> u64 as *const u8
        }
        RuntimeOp::Subtract => {
            rts_core_rwk::entry::subtract as extern "C" fn(u64, u64) -> u64 as *const u8
        }
        RuntimeOp::Multiply => {
            rts_core_rwk::entry::multiply as extern "C" fn(u64, u64) -> u64 as *const u8
        }
        RuntimeOp::Divide => {
            rts_core_rwk::entry::divide as extern "C" fn(u64, u64) -> u64 as *const u8
        }
        RuntimeOp::Remainder => {
            rts_core_rwk::entry::remainder as extern "C" fn(u64, u64) -> u64 as *const u8
        }
        RuntimeOp::Less => {
            rts_core_rwk::entry::less as extern "C" fn(u64, u64) -> bool as *const u8
        }
        RuntimeOp::LessEqual => {
            rts_core_rwk::entry::less_equal as extern "C" fn(u64, u64) -> bool as *const u8
        }
        RuntimeOp::Greater => {
            rts_core_rwk::entry::greater as extern "C" fn(u64, u64) -> bool as *const u8
        }
        RuntimeOp::GreaterEqual => {
            rts_core_rwk::entry::greater_equal as extern "C" fn(u64, u64) -> bool as *const u8
        }
        RuntimeOp::ObjectNew => {
            rts_core_rwk::entry::object_new as extern "C" fn() -> u64 as *const u8
        }
        RuntimeOp::GetProperty => {
            rts_core_rwk::entry::get_property as extern "C" fn(u64, i64) -> u64 as *const u8
        }
        RuntimeOp::SetProperty => {
            rts_core_rwk::entry::set_property as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }
        RuntimeOp::ClosureNew => {
            rts_core_rwk::entry::closure_new as extern "C" fn(i64, u64) -> u64 as *const u8
        }
        // The cast is the arity agreement, written out. Six parameters: the
        // callee, the receiver, and `ARGUMENT_SLOTS` arguments — and the
        // assertion below is what makes that sentence checkable rather than a
        // comment that was true once.
        RuntimeOp::Call => {
            rts_core_rwk::entry::call as extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64
                as *const u8
        }
        // The argument is which literal, not the text: an `i64` index into the
        // table the run seeds. Writing the cast out is what makes a change to
        // that decision a type error here.
        RuntimeOp::StringConst => {
            rts_core_rwk::entry::string_const as extern "C" fn(i64) -> u64 as *const u8
        }
        RuntimeOp::TypeOf => {
            rts_core_rwk::entry::type_of as extern "C" fn(u64) -> u64 as *const u8
        }
    })
}

/// Which runtime entry an operation the language named actually is.
///
/// # Why this exists rather than the compiler reading the descriptor
///
/// `#[rtse::entry]` already derives a symbol and an ABI shape from each Rust
/// signature, so `rts-core-rwk` publishes the truth about both. `rts-codegen`
/// states them again by hand — and that is not laziness, it is the layering: the
/// language decides *membership* of the entry-point set, which is a language
/// judgement, and it must be able to target **a** runtime rather than **the**
/// one in this workspace. Depending on `rts-core-rwk` to read the descriptor
/// would make the compiler unable to compile against any other.
///
/// What the doctrine actually requires of a restatement is that something
/// **check** it, and until this function nothing did. A skew between what the
/// compiler emitted and what the runtime defined is not a link error: the symbol
/// resolves, the call is laid out to the compiler's shape, and the callee reads
/// its arguments to a different one. Silent, and corrupt.
///
/// So this is the mapping, and [`agree`] is the check. Adding an operation to
/// the compiler without one here fails to compile, which is the same property
/// `address_of` has and for the same reason.
fn entry_of(op: RuntimeOp) -> CoreEntry {
    match op {
        RuntimeOp::Add => CoreEntry::Add,
        RuntimeOp::StrictEquals => CoreEntry::StrictEquals,
        RuntimeOp::ToBoolean => CoreEntry::ToBoolean,
        RuntimeOp::NumberToString => CoreEntry::NumberToString,
        RuntimeOp::Subtract => CoreEntry::Subtract,
        RuntimeOp::Multiply => CoreEntry::Multiply,
        RuntimeOp::Divide => CoreEntry::Divide,
        RuntimeOp::Remainder => CoreEntry::Remainder,
        RuntimeOp::Less => CoreEntry::Less,
        RuntimeOp::LessEqual => CoreEntry::LessEqual,
        RuntimeOp::Greater => CoreEntry::Greater,
        RuntimeOp::GreaterEqual => CoreEntry::GreaterEqual,
        RuntimeOp::ObjectNew => CoreEntry::ObjectNew,
        RuntimeOp::GetProperty => CoreEntry::GetProperty,
        RuntimeOp::SetProperty => CoreEntry::SetProperty,
        RuntimeOp::ClosureNew => CoreEntry::ClosureNew,
        RuntimeOp::Call => CoreEntry::Call,
        RuntimeOp::StringConst => CoreEntry::StringConst,
        RuntimeOp::TypeOf => CoreEntry::TypeOf,
    }
}

/// The compiler and the runtime describe an operation the same way.
///
/// Checked for every operation a compilation actually declared, before anything
/// is placed. Both halves matter and they fail differently: a symbol skew is a
/// missing symbol at placement, which is loud, and a **shape** skew is a call
/// laid out one way and read another, which is not.
fn agree(op: RuntimeOp) -> Result<(), HostError> {
    let described = entry_of(op).describe();
    if op.symbol() != described.symbol {
        return Err(HostError::Malformed(format!(
            "the compiler calls {:?} `{}` and the runtime defines `{}`",
            op,
            op.symbol(),
            described.symbol
        )));
    }
    // Compared position by position rather than as whole signatures, because
    // the two layers speak different vocabularies for the same fact: the IR
    // says `Repr`, the ABI says `AbiType`, and an entry point's parameters are
    // all scalars. A descriptor holding an aggregate or a slice here is not a
    // mismatch to report — it is an entry point the compiler cannot call at
    // all, so it is named separately.
    let shape = describe(op.signature().params, described.params)
        .and_then(|()| describe(op.signature().returns, described.returns));
    if let Err(reason) = shape {
        return Err(HostError::Malformed(format!(
            "the compiler and the runtime disagree about the shape of `{}`: {reason}",
            op.symbol(),
        )));
    }
    Ok(())
}

/// One side's representations against the other's ABI types.
fn describe(ours: Vec<Repr>, theirs: &[AbiType]) -> Result<(), String> {
    if ours.len() != theirs.len() {
        return Err(format!(
            "{} positions against {}",
            ours.len(),
            theirs.len()
        ));
    }
    for (at, (ours, theirs)) in ours.iter().zip(theirs).enumerate() {
        match theirs {
            AbiType::Scalar(theirs) if theirs == ours => {}
            AbiType::Scalar(theirs) => {
                return Err(format!("position {at} is {ours:?} against {theirs:?}"));
            }
            other => {
                return Err(format!(
                    "position {at} is {ours:?} against {other:?}, which is not a \
                     scalar and so is not something a call site can lay out"
                ));
            }
        }
    }
    Ok(())
}

/// The two sides agree about how many arguments a compiled call carries.
///
/// `rts-codegen` decides it, because which convention compiled code uses is a
/// fact about what that crate emits. `rts-core-rwk` restates it, because it is
/// what performs the call. Neither can see the other, and this crate is the one
/// that may name both — so this is where a disagreement becomes a refusal.
///
/// A `const` assertion rather than a test: a test that is not run proves
/// nothing, and this one cannot fail to be checked because the crate does not
/// compile without it.
const _: () = assert!(
    rts_codegen::runtime::ARGUMENT_SLOTS == rts_core_rwk::entry::ARGUMENT_SLOTS,
    "the compiler and the runtime disagree about how many arguments a call \
     carries, which is a jump with a corrupt stack rather than a wrong answer"
);

/// The runtime's implementation of one of the machine's own entry points.
///
/// Separate from `address_of` because the two are different contracts. That one
/// serves `RuntimeOp`, which `rts-codegen` states and this crate resolves; this
/// one serves `RtEntry`, which `rts-cranelift` states and emits itself. A match
/// missing an arm there means the language named something the runtime lacks; a
/// match missing an arm here means the machine emits an instruction whose entry
/// point nobody supplied, which is a crash in compiled code.
fn machine_entry(entry: RtEntry) -> *const u8 {
    match entry {
        RtEntry::Alloc => rts_core_rwk::entry::alloc as extern "C" fn(i64, i64) -> u64 as *const u8,
        RtEntry::CacheResolve => {
            rts_core_rwk::entry::cache_resolve as extern "C" fn(u64, i64, i64) -> i64 as *const u8
        }
        // The rest are emitted by instructions this compiler does not produce:
        // the write barrier by a store the collector must learn of, the promise
        // operations by `await`, the throw by an escaping exception. Each
        // arrives with the phase that emits it.
        RtEntry::WriteBarrier => {
            rts_core_rwk::entry::write_barrier as extern "C" fn(u64, u64) as *const u8
        }
        RtEntry::PromiseNew
        | RtEntry::PromiseSettle
        | RtEntry::PromiseAwait
        | RtEntry::Throw => std::ptr::null(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_the_compiler_names_is_the_one_the_runtime_defines() {
        // Checked for ALL of them rather than for what some program happened to
        // call, because the failure this catches is in the operation nobody
        // exercised yet: the compiler states a symbol and a shape by hand, the
        // runtime derives both from a Rust signature, and until this existed
        // nothing compared them.
        //
        // A symbol skew is loud — a missing symbol at placement. A shape skew is
        // not: the call is laid out to the compiler's answer and the callee
        // reads its arguments to a different one, which is a corrupt call that
        // links and runs.
        for op in RuntimeOp::ALL {
            agree(*op).unwrap_or_else(|error| {
                panic!("{op:?} does not match what the runtime defines: {error:?}")
            });
        }
    }

    #[test]
    fn the_two_lists_are_the_same_length() {
        // `entry_of` is exhaustive over `RuntimeOp`, so an operation added to
        // the compiler cannot be forgotten here. The other direction has no
        // such check: an entry point added to the runtime and never named by
        // the language is legal — it is simply unused — but the counts being
        // equal is what says that is not the case today, so a divergence is
        // visible rather than assumed.
        assert_eq!(
            RuntimeOp::ALL.len(),
            rts_core_rwk::entry::CORE_ENTRY_COUNT,
            "the compiler names {} operations and the runtime numbers {}",
            RuntimeOp::ALL.len(),
            rts_core_rwk::entry::CORE_ENTRY_COUNT
        );
    }
}
