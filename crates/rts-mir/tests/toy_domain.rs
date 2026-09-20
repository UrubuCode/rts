//! A second domain, so that the parameterisation is exercised rather than
//! asserted.
//!
//! README rule 10, and `docs/engine/a-second-language.md`'s sentence behind it:
//! *"a boundary with one client on each side is indistinguishable from no
//! boundary at all"*. With only JavaScript above this crate, it acquires
//! JavaScript by accident — a `Prim` gets special-cased, a truth rule gets
//! assumed, an integer becomes "a number that happens to be whole".
//!
//! So this file implements a domain deliberately unlike the first one, on the
//! three axes `a-second-language.md` measured as not neutral:
//!
//! - **integer and float are distinct types**, not one numeric type with an
//!   optimisation;
//! - **two false values**, not seven: `Nil` and `Bool(false)`. A zero is TRUE.
//! - it has a type the other language does not have at all.
//!
//! When a pass cannot be written against this, that is the finding.

use rts_cranelift::fault::Position;
use rts_mir::cfg::{Callee, EntryId, FuncBuilder};
use rts_mir::domain::Domain;
use rts_mir::{Assertion, Const, Effect, Op, PointId, Prim, Terminator, Tier, verify};

/// What the toy language knows about a value.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Toy {
    /// No path arrives.
    Nothing,
    /// Absent, and one of the two false values.
    Nil,
    /// A truth value, known or not.
    Bool(Option<bool>),
    /// A whole number. A type of its own, which is the point.
    Integer,
    /// Not whole. Distinct from [`Toy::Integer`], and joining the two does NOT
    /// give a single numeric type — it gives `Anything`, because this language has
    /// no supertype of the two.
    Float,
    /// A string.
    Text,
    /// Nothing is known.
    Anything,
}

/// The primitives this language declares, by the index the IR carries.
const ADD_INTEGERS: Prim = Prim(0);
const DIVIDE: Prim = Prim(1);
const CONCATENATE: Prim = Prim(2);
/// Whether two values are equal, which this language answers as a `Bool`.
const EQUALS: Prim = Prim(3);

/// The assertions this language's guards make.
const IS_INTEGER: Assertion = Assertion(0);
const IS_TEXT: Assertion = Assertion(1);

struct ToyDomain;

impl Domain for ToyDomain {
    type Type = Toy;

    fn top(&self) -> Toy {
        Toy::Anything
    }

    fn bottom(&self) -> Toy {
        Toy::Nothing
    }

    fn join(&self, left: &Toy, right: &Toy) -> Toy {
        match (left, right) {
            (Toy::Nothing, other) | (other, Toy::Nothing) => other.clone(),
            (Toy::Bool(one), Toy::Bool(two)) => match one == two {
                true => Toy::Bool(*one),
                false => Toy::Bool(None),
            },
            // The axis that matters: an integer joined with a float is NOT a
            // number here. A domain that answered `Number` would be the other
            // language's lattice wearing this one's names.
            (one, two) if one == two => one.clone(),
            _ => Toy::Anything,
        }
    }

    fn of_const(&self, value: &Const) -> Toy {
        match value {
            Const::Int(_) => Toy::Integer,
            Const::Float(_) => Toy::Float,
            Const::Bool(truth) => Toy::Bool(Some(*truth)),
            // The language's own table. Index zero is its `nil`.
            Const::Declared(0) => Toy::Nil,
            Const::Declared(_) => Toy::Anything,
        }
    }

    fn transfer(&self, prim: Prim, args: &[Toy]) -> Toy {
        match prim {
            ADD_INTEGERS => match args {
                [Toy::Integer, Toy::Integer] => Toy::Integer,
                _ => Toy::Anything,
            },
            // Division answers a float even of two integers, which is this
            // language's rule and not the other's.
            DIVIDE => Toy::Float,
            CONCATENATE => Toy::Text,
            EQUALS => Toy::Bool(None),
            _ => Toy::Anything,
        }
    }

    fn of_entry(&self, _entry: EntryId) -> Toy {
        Toy::Anything
    }

    fn narrow(&self, assertion: Assertion, of: &Toy) -> Toy {
        match assertion {
            IS_INTEGER => Toy::Integer,
            IS_TEXT => Toy::Text,
            _ => of.clone(),
        }
    }

    /// This language's coercion rule, which is not the other's.
    ///
    /// Adding two integers is arithmetic and nothing else. Adding anything else
    /// reaches this language's metatable, which is code a program wrote — so the
    /// boundary between pure and not is drawn at a DIFFERENT place here, and that
    /// is the point of the method being on the domain.
    fn effect_of(&self, prim: Prim, args: &[Toy]) -> Effect {
        match prim {
            ADD_INTEGERS => match args {
                [Toy::Integer, Toy::Integer] => Effect::PURE,
                _ => Effect::CALLS_USER.and(Effect::THROWS),
            },
            DIVIDE => match args.iter().all(|held| matches!(held, Toy::Integer | Toy::Float)) {
                true => Effect::PURE,
                false => Effect::CALLS_USER.and(Effect::THROWS),
            },
            // Building a string allocates however it is reached.
            CONCATENATE => Effect::ALLOCATES,
            EQUALS => Effect::PURE,
            _ => Effect::CALLS_USER.and(Effect::THROWS).and(Effect::WRITES),
        }
    }

    fn truth_of(&self, of: &Toy) -> Option<bool> {
        match of {
            Toy::Nil => Some(false),
            Toy::Bool(known) => *known,
            // Two false values, so everything else is true -- INCLUDING a zero and
            // an empty string, which is where the other language answers
            // differently.
            Toy::Integer | Toy::Float | Toy::Text => Some(true),
            Toy::Nothing | Toy::Anything => None,
        }
    }
}

fn at() -> Position {
    Position::default()
}

#[test]
fn a_constant_takes_its_type_from_the_language_and_not_from_the_ir() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let whole = build.push(Op::Const(Const::Int(2)), Effect::PURE, at());
    let fraction = build.push(Op::Const(Const::Float(0.5)), Effect::PURE, at());
    build.end(Terminator::Return(Some(whole)));
    let func = build.finish();
    assert_eq!(verify(&func), Ok(()));

    let types = rts_mir::infer::infer(&func, &ToyDomain);
    assert_eq!(*types.of(whole), Toy::Integer);
    assert_eq!(*types.of(fraction), Toy::Float);
}

/// The axis `a-second-language.md` measured. A shared lattice would have had to
/// pick one of the two answers.
#[test]
fn an_integer_joined_with_a_float_is_not_a_number_in_this_language() {
    let domain = ToyDomain;
    assert_eq!(domain.join(&Toy::Integer, &Toy::Float), Toy::Anything);
    assert_eq!(domain.join(&Toy::Integer, &Toy::Integer), Toy::Integer);
    // And `bottom` is the identity, which is what `infer` relies on to seed a
    // block nothing has reached yet.
    assert_eq!(domain.join(&Toy::Nothing, &Toy::Text), Toy::Text);
    assert_eq!(domain.join(&Toy::Text, &Toy::Nothing), Toy::Text);
}

#[test]
fn a_zero_is_true_here_and_the_ir_has_no_opinion() {
    let domain = ToyDomain;
    assert_eq!(domain.truth_of(&Toy::Integer), Some(true));
    assert_eq!(domain.truth_of(&Toy::Text), Some(true));
    assert_eq!(domain.truth_of(&Toy::Nil), Some(false));
    assert_eq!(domain.truth_of(&Toy::Anything), None);
}

/// A loop: the parameter's type is the join across the back edge, which is the
/// answer no syntax-directed traversal can compute.
#[test]
fn a_loop_parameter_reaches_a_fixed_point() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let header = build.block();
    let carried = build.param(header);
    let start = build.push(Op::Const(Const::Int(0)), Effect::PURE, at());
    build.end(Terminator::Jump {
        target: header,
        args: vec![start],
    });
    build.switch_to(header);
    let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, at());
    let next = build.push(
        Op::Prim {
            prim: ADD_INTEGERS,
            args: vec![carried, one],
        },
        Effect::PURE,
        at(),
    );
    build.end(Terminator::Jump {
        target: header,
        args: vec![next],
    });
    let func = build.finish();
    assert_eq!(verify(&func), Ok(()));

    let types = rts_mir::infer::infer(&func, &ToyDomain);
    // Integer in, integer added, integer back: the fixed point is `Integer` and
    // not `Anything`, which is the whole value of iterating.
    assert_eq!(*types.of(carried), Toy::Integer);
    assert_eq!(*types.of(next), Toy::Integer);
}

/// And the same loop where the back edge carries a different type: the fixed
/// point widens, and it must widen to this language's answer.
#[test]
fn a_loop_whose_back_edge_disagrees_widens_to_the_languages_own_top() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let header = build.block();
    let carried = build.param(header);
    let start = build.push(Op::Const(Const::Int(0)), Effect::PURE, at());
    build.end(Terminator::Jump {
        target: header,
        args: vec![start],
    });
    build.switch_to(header);
    let divided = build.push(
        Op::Prim {
            prim: DIVIDE,
            args: vec![carried],
        },
        Effect::PURE,
        at(),
    );
    build.end(Terminator::Jump {
        target: header,
        args: vec![divided],
    });
    let func = build.finish();
    assert_eq!(verify(&func), Ok(()));

    let types = rts_mir::infer::infer(&func, &ToyDomain);
    assert_eq!(*types.of(divided), Toy::Float);
    // Integer from the entry, float from the latch, and this language has no
    // supertype of the two.
    assert_eq!(*types.of(carried), Toy::Anything);
}

/// What a guard buys, which is the only reason the specialised tier knows more.
#[test]
fn a_guard_narrows_what_the_domain_says_it_narrows() {
    let mut build = FuncBuilder::new(Tier::Specialised);
    let unknown = build.push(
        Op::Call {
            callee: Callee::Entry(EntryId(0)),
            receiver: None,
            args: Vec::new(),
        },
        Effect::CALLS_USER,
        at(),
    );
    let proved = build.push(
        Op::Guard {
            assertion: IS_INTEGER,
            on: unknown,
            point: PointId(0),
        },
        Effect::PURE,
        at(),
    );
    let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, at());
    let sum = build.push(
        Op::Prim {
            prim: ADD_INTEGERS,
            args: vec![proved, one],
        },
        Effect::PURE,
        at(),
    );
    build.end(Terminator::Return(Some(sum)));
    let func = build.finish();
    assert_eq!(verify(&func), Ok(()));

    let types = rts_mir::infer::infer(&func, &ToyDomain);
    assert_eq!(*types.of(unknown), Toy::Anything);
    assert_eq!(*types.of(proved), Toy::Integer);
    // And the addition is now known, which is the point of the guard: without it
    // the primitive's transfer answers `Anything`.
    assert_eq!(*types.of(sum), Toy::Integer);
}

/// The IR must not know that `CONCATENATE` produces text, nor that `Prim(2)` is
/// concatenation at all. Renumbering the table changes nothing structural.
#[test]
fn a_primitive_is_an_index_and_the_ir_never_interprets_it() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let text = build.push(
        Op::Prim {
            prim: CONCATENATE,
            args: Vec::new(),
        },
        Effect::ALLOCATES,
        at(),
    );
    let compared = build.push(
        Op::Prim {
            prim: EQUALS,
            args: vec![text, text],
        },
        Effect::PURE,
        at(),
    );
    build.end(Terminator::Return(Some(compared)));
    let func = build.finish();
    assert_eq!(verify(&func), Ok(()));

    let types = rts_mir::infer::infer(&func, &ToyDomain);
    assert_eq!(*types.of(text), Toy::Text);
    assert_eq!(*types.of(compared), Toy::Bool(None));
    // The effect summary travels with the instruction, and allocating is what
    // stops a pass hoisting this out of a loop.
    assert!(func.inst(rts_mir::InstId(0)).effect.may_collect());
}

/// The effect-refining pass over THIS language, which is what makes it a pass and
/// not a JavaScript feature living in a shared crate.
///
/// The loop is the same shape the other language's is, and the answer comes from
/// this language's own coercion boundary: adding two integers is arithmetic, and
/// the lowering could not know the carried value was one.
#[test]
fn the_effect_pass_narrows_through_this_languages_own_rule() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let header = build.block();
    let carried = build.param(header);
    let start = build.push(Op::Const(Const::Int(0)), Effect::PURE, at());
    build.end(Terminator::Jump {
        target: header,
        args: vec![start],
    });
    build.switch_to(header);
    let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, at());
    // As a lowering would record it: the header's type was unknown at this point.
    let next = build.push(
        Op::Prim {
            prim: ADD_INTEGERS,
            args: vec![carried, one],
        },
        Effect::CALLS_USER.and(Effect::THROWS),
        at(),
    );
    build.end(Terminator::Jump {
        target: header,
        args: vec![next],
    });
    let mut func = build.finish();
    assert_eq!(verify(&func), Ok(()));

    let refined = rts_mir::passes::refine_effects(&mut func, &ToyDomain);
    assert_eq!(refined.narrowed, 1);
    assert_eq!(refined.refused, 0);
    assert!(func.inst(rts_mir::InstId(2)).effect.is_pure());
    assert_eq!(verify(&func), Ok(()));
}

/// And an allocation is NOT narrowed away, because allocating is what the operation
/// does rather than something its operands decide.
#[test]
fn an_allocation_survives_the_pass() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let text = build.push(
        Op::Prim {
            prim: CONCATENATE,
            args: Vec::new(),
        },
        Effect::ALLOCATES,
        at(),
    );
    build.end(Terminator::Return(Some(text)));
    let mut func = build.finish();

    let refined = rts_mir::passes::refine_effects(&mut func, &ToyDomain);
    assert_eq!(refined.narrowed, 0);
    assert!(func.inst(rts_mir::InstId(0)).effect.may_collect());
}
