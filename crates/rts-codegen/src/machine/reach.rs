//! Does a graph reach the machine, and what stopped it if not?
//!
//! Apart from the boundary itself because the ceiling said so, and along a real seam: this
//! is the INSTRUMENT. It builds a signature, threads the state a program shares, and
//! answers one question about one function -- none of which the boundary needs to know.

use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::repr::Repr;

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
        }
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
/// # The signature comes from the LANGUAGE, one parameter at a time
///
/// Whatever the lattice proved about the graph's entry parameters is what the machine
/// function is declared to take, which is the agreement `param_repr` exists to state. A
/// caller that supplied its own would be measuring its own guess.
pub fn reaches_machine(
    func: &rts_mir::cfg::Func,
    domain: &Js,
    generic: Option<rts_mir::cfg::FuncId>,
    shared: &mut Shared,
    names: &mut crate::names::Names,
) -> Result<(), rts_mir::lower::Unlowerable> {
    use rts_cranelift::ir::{Function, Signature};
    use rts_cranelift::types::TypeRegistry;

    let types = TypeRegistry::new();
    let inferred = rts_mir::infer::infer(func, domain);
    let params: Vec<Repr> = func
        .block(func.entry())
        .params
        .iter()
        .map(|held| JsMachine::repr_of(inferred.of(*held)).unwrap_or(Repr::Tagged))
        .collect();
    // AND THE RETURN, from the same place. A function with no `Return` carrying a value
    // returns nothing, which is a signature with no results rather than one with an
    // invented result.
    let returns: Vec<Repr> = func
        .block_ids()
        .filter_map(|held| match func.block(held).terminator {
            Some(rts_mir::cfg::Terminator::Return(Some(value))) => Some(value),
            _ => None,
        })
        .next()
        .map(|held| JsMachine::repr_of(inferred.of(held)).unwrap_or(Repr::Tagged))
        .into_iter()
        .collect();
    // THE LANGUAGE'S CALLING CONVENTION, which `emit/function.rs` fixes and
    // `rts_core::entry::functions::invoke` calls on: parameter 0 is the environment and
    // parameter 1 is the receiver, both runtime values and therefore tagged. The
    // program's own parameters follow.
    //
    // Adopted here rather than left out, because a signature without them is one nothing
    // in this engine can call -- and the instrument would be measuring a function shape
    // that could never run.
    let leading = vec![Repr::Tagged, Repr::Tagged];
    let signature = Signature {
        params: leading.iter().copied().chain(params).collect(),
        returns,
        ..Signature::default()
    };
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
    // THE GRAPH'S PARAMETERS ARE THE PROGRAM'S, so they map onto the TAIL of the
    // signature. Handing the whole list would bind the program's first parameter to the
    // environment, which is the kind of off-by-two that compiles.
    let mut ops = match twin {
        Some(twin) => JsMachine::falling_to(domain, inferred, twin),
        None => JsMachine::new(domain, inferred),
    }
    .declaring_in(shared)
    .naming_with(names)
    .with_incoming(&start);
    rts_mir::lower::lower(func, &mut into, &mut ops, &start[2..])
}
