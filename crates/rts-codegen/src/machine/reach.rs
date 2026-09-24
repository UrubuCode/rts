//! Does a graph reach the machine, and what stopped it if not?
//!
//! Apart from the boundary itself because the ceiling said so, and along a real seam: this
//! is the INSTRUMENT. It builds a signature, threads the state a program shares, and
//! answers one question about one function -- none of which the boundary needs to know.

use rts_cranelift::ir::FuncRegistry;

use rts_mir::lower::MachineOps as _;

use super::JsMachine;
use crate::domain::Js;
/// What every function of one module shares.
///
/// A `FuncId` is an agreement about a symbol, so it belongs to the PROGRAM and not to a
/// function -- which is how `rts-host::graph` builds a real one: `FuncRegistry` and
/// `RuntimeCalls` are created once and every file shares them. A registry per function
/// would hand each its own id for one symbol and nothing would notice.
pub struct Shared {
    /// Every function a call may name.
    pub funcs: rts_cranelift::ir::FuncRegistry,
    /// Which runtime operations have been declared, so each is declared once.
    pub calls: crate::runtime::RuntimeCalls,
    /// Every string the program holds, numbered as the runtime numbers it.
    ///
    /// Here and not per function for the same reason the registry is: a string's index
    /// is an agreement with the runtime, which holds ONE table. Two functions numbering
    /// their own would reach the wrong string rather than failing.
    pub literals: crate::runtime::Literals,
    /// What the machine calls each property, numbered once per program.
    ///
    /// `rts_cranelift::shape::KeyRegistry` hands out numbers and records no names, which
    /// is deliberate on its side: a key it understood would be a key it could be wrong
    /// about. The PAIRING of a name to a key lives on `Names`, and CLAUDE.md names this as
    /// the correct shape -- two tables of different lifetimes minting from ONE registry.
    pub keys: rts_cranelift::shape::KeyRegistry,
    /// The tags a value's representation is built from.
    ///
    /// Program-scoped like every other numbering here: a tag is an agreement about bits,
    /// and two registries would encode one singleton two ways.
    pub tags: rts_cranelift::tags::TagRegistry,
    /// What a value IS, in the tags above.
    ///
    /// Built from them rather than beside them, which is why the two are one field apart
    /// and never constructed separately.
    pub model: crate::values::ValueModel,
    /// The machine's id for each function of the module, indexed by its MIR number.
    ///
    /// What a closure is made of is a CODE ADDRESS, and an address exists only for a
    /// function the registry declared -- so a function value was refused until the
    /// module's functions were numbered on the machine's side as well as on the MIR's.
    /// Every one is declared with the convention's one signature, which is what makes it
    /// possible to number them before any is lowered: nothing about the body decides it.
    pub module: Vec<rts_cranelift::ir::FuncId>,
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

impl Shared {
    /// Every numbering a program agrees with the runtime about, empty.
    ///
    /// Not derived, because the value model is DECLARED from the tag registry -- the two
    /// are one fact in two pieces, and a `Default` that made them separately would build a
    /// model against tags nothing else uses.
    pub fn new() -> Self {
        let mut tags = rts_cranelift::tags::TagRegistry::new();
        let model = crate::values::ValueModel::declare(&mut tags);
        Self {
            funcs: rts_cranelift::ir::FuncRegistry::new(),
            calls: crate::runtime::RuntimeCalls::new(),
            literals: crate::runtime::Literals::new(),
            keys: rts_cranelift::shape::KeyRegistry::new(),
            tags,
            model,
            module: Vec::new(),
        }
    }

    /// Declares a machine function for each of a module's `count` functions, in order.
    pub fn number_module(&mut self, count: usize) {
        let signature = self.funcs.declare_signature(crate::emit::convention());
        self.module = (0..count)
            .map(|_| self.funcs.declare_function(signature))
            .collect();
    }
}

/// Does this graph reach the machine, and what stopped it if not?
///
/// # Why this is here and not only in a test
///
/// Because it is the only honest answer to "how far along is this stage". The share of
/// functions that lower to a GRAPH is 88% and says nothing about whether any of them
/// becomes code: a graph is refused at the machine boundary for reasons the graph itself
/// cannot show — an operand nothing proved, an entry point with no address, a region
/// with no tag.
///
/// So the two numbers are different questions and the second one is the one a reader
/// wants. Having it in the library rather than in a test is what lets `rts mir` report
/// it, and an instrument that can only be run by `cargo test` is an instrument nobody
/// runs.
///
/// # The signature is the CONVENTION, and it used to be the proof
///
/// It was built from what the lattice proved about the graph's entry parameters and its
/// return, one at a time -- which is a signature nothing in this engine can call. The
/// runtime enters a function through `rts_core::entry::functions::invoke`, a closure is a
/// code address the runtime calls on those terms, and `emit/function.rs` states them:
/// the environment, the receiver, a fixed number of argument slots, one result, every one
/// tagged. A function declared any other way could not be the code of a closure, so the
/// instrument was measuring functions that could never be called.
///
/// So it is `emit::convention()`, reused rather than restated, and a proved return is
/// widened on the way out. Entry parameters were tagged under the old rule anyway --
/// nothing is proved about a parameter before its guard -- so what changes is the return
/// and the slot count.
pub fn reaches_machine(
    func: &rts_mir::cfg::Func,
    domain: &Js,
    generic: Option<rts_mir::cfg::FuncId>,
    shared: &mut Shared,
    names: &mut crate::names::Names,
) -> Result<(), rts_mir::lower::Unlowerable> {
    use rts_cranelift::ir::Function;
    use rts_cranelift::types::TypeRegistry;

    let types = TypeRegistry::new();
    let inferred = rts_mir::infer::infer(func, domain);
    // A PARAMETER PAST THE SLOTS has nowhere to arrive: the convention fixes how many a
    // call hands over, and `runtime/mod.rs` says why. Refused rather than read as
    // `undefined` forever, which is what `emit/function.rs` decided for the same case.
    let written = func.block(func.entry()).params.len();
    if written > crate::runtime::ARGUMENT_SLOTS {
        return Err(rts_mir::lower::Unlowerable::Language(format!(
            "{written} parameters, and the convention has {} slots",
            crate::runtime::ARGUMENT_SLOTS
        )));
    }
    let signature = crate::emit::convention();
    // THE GENERIC TWIN, when the caller has one. Declared with the same signature
    // because a fall hands over the same arguments and answers what the other tier
    // answers -- the two bodies of one function agree about their shape by definition.
    let twin = generic.map(|_| {
        let sig = shared.funcs.declare_signature(signature.clone());
        shared.funcs.declare_function(sig)
    });
    let mut machine = Function::new(signature);
    let entry = machine.entry;
    let start: Vec<_> = machine
        .block(entry)
        .expect("a function has an entry block")
        .params
        .clone();
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut machine, &types, entry);
    // ONE REGISTRY FOR THE WHOLE MODULE, which is how `rts-host::graph` builds a real
    // program: `funcs` and `calls` are created once and every file and function share
    // them. A registry per function would give each one its own `FuncId` for one symbol,
    // and the instrument would never notice two functions failing to agree.
    //
    // Handed over whether or not there is a twin to fall to. Those were
    // mutually exclusive while the twin carried its own reference to the registry, which
    // is a limit nothing but that field imposed -- and it made the instrument measure a
    // falling function without entry points and a non-falling one with them.
    // THE GRAPH'S PARAMETERS ARE THE PROGRAM'S, so they map onto the slots after the
    // convention's two. Handing the whole list would bind the program's first parameter
    // to the environment, which is the kind of off-by-two that compiles; a slot the
    // program did not name is simply not read.
    let mut ops = match twin {
        Some(twin) => JsMachine::falling_to(domain, inferred, twin),
        None => JsMachine::new(domain, inferred),
    }
    .declaring_in(shared)
    .naming_with(names)
    .with_incoming(&start);
    rts_mir::lower::lower(func, &mut into, &mut ops, &start[2..2 + written])?;
    drop(ops);
    drop(into);
    // THE MACHINE'S OWN VERIFIER IS THE LAST WORD, and this instrument did not ask it.
    // A function the builder accepted instruction by instruction can still be one the
    // verifier refuses as a whole -- a value used where it does not dominate, a jump
    // whose arguments disagree with the target's -- and counting it as reaching the
    // machine would be counting a program the code generator would reject.
    let refused = rts_cranelift::verify(&machine, &types, &shared.funcs);
    match refused.first() {
        None => Ok(()),
        Some(first) => Err(rts_mir::lower::Unlowerable::Machine(format!(
            "the machine's verifier refused it: {first:?}"
        ))),
    }
}
