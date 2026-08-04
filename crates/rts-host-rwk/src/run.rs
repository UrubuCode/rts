//! Source text in, callable code out.

use rts_codegen::emit::{Ctx, emit_body};
use rts_codegen::names::Names;
use rts_codegen::parse::parse_script;
use rts_codegen::runtime::{RuntimeCalls, RuntimeOp};
use rts_codegen::syntax::{FunctionBody, ModuleItem, StmtKind};
use rts_codegen::values::ValueModel;
use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::tags::TagRegistry;
use rts_cranelift::target::{InMemory, Placing, Visibility, place_in_memory};
use rts_cranelift::types::TypeRegistry;

use crate::link::HostError;

/// The name a compiled script is placed under.
///
/// Not derived from anything in the source: a script has no name, and inventing
/// one from a file path would make the symbol depend on where the file was.
const SCRIPT: &str = "__rts_script";

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
    entry: extern "C" fn() -> u64,
    /// What the compiler decided the singletons are numbered.
    model: ValueModel,
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
    pub fn run(&self) -> u64 {
        let singletons = crate::link::singletons_for(&self.model);
        let context = rts_core_rwk::entry::Context::new(singletons);
        let (_context, value) =
            rts_core_rwk::entry::with_context(context, || (self.entry)());
        value
    }

    /// What the compiler numbered the singletons, for a caller reading a result.
    pub fn model(&self) -> &ValueModel {
        &self.model
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

    let script_id = {
        let mut ctx = Ctx::new(&model, &mut funcs, &mut calls);
        let func = emit_body(body, &[], &types, &mut ctx)?;
        (func, ())
    };
    let (func, ()) = script_id;

    // The script's own function is declared after emission, because emission is
    // what decides its signature.
    let script_sig = funcs.declare_signature(func.signature.clone());
    let script = funcs.declare_function(script_sig);

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
    placing.push(Placing {
        id: script,
        name: SCRIPT,
        visibility: Visibility::Exported,
        body: Some(&func),
    });

    let outside: Vec<(&str, *const u8)> = expected
        .iter()
        .map(|(op, _)| Ok((op.symbol(), address_of(*op)?)))
        .collect::<Result<_, HostError>>()?;

    // Asked before anything is placed, and it earns its line immediately: the
    // first program that returned `1 === 1` handed back a machine boolean where
    // its signature declared a tagged value, and went straight to the code
    // generator because nothing here had asked.
    //
    // The verifier had the check all along. Skipping it made the machine's
    // answer to "is this well formed" unavailable at the one moment it was
    // about to matter.
    let complaints = rts_cranelift::verify::verify(&func, &types, &funcs);
    if !complaints.is_empty() {
        return Err(HostError::Malformed(format!("{complaints:?}")));
    }

    // SAFETY: every address comes from `address_of`, which returns the runtime's
    // own entry points — functions in this binary, alive for the whole process,
    // whose signatures the runtime derives from the same Rust definitions the
    // compiler was told about.
    let placed = unsafe { place_in_memory(&placing, &outside, &funcs, &types)? };

    let address = placed
        .address_of(script)
        .expect("the script was placed with a body");
    // SAFETY: the signature emitted for a script takes nothing and returns one
    // tagged value, which is what `emit_body` builds and what this transmute
    // claims.
    let entry: extern "C" fn() -> u64 = unsafe { std::mem::transmute(address) };

    Ok(Compiled {
        placed,
        entry,
        model,
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
    })
}
