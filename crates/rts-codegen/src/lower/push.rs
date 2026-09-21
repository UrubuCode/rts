//! The small pushers: a constant, a singleton, a declared index, a primitive.
//!
//! Apart from the lowering because `mod.rs` passed its 1000-line ceiling a third time,
//! and this is the cohesive remainder: every function here PUSHES one instruction and
//! records what is known about its result. None of them reads the tree beyond the
//! position it needs, and none of them decides control flow.
//!
//! The one that carries a decision is [`Lowering::prim`], and the decision is where
//! `domain::Js::effect_of` earns its shape: the same operator is `PURE` over two proven
//! numbers and `CALLS_USER|THROWS` over two unknowns, so what a pass may later do with
//! an instruction is settled here, from what is known here.

use rts_mir::Domain;
use rts_mir::Effect;
use rts_mir::cfg::{Const, Op, ValueId};

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim, Type};
use crate::syntax::{Expr, Literal, Stmt};
use crate::values::Singleton;

impl Lowering<'_> {
    pub(super) fn literal(&mut self, literal: &Literal, at: &Expr) -> Result<ValueId, Unsupported> {
        let value = match literal {
            // An integral number that fits is an `Int::Int`, so the domain can
            // answer `Int32` for it and an addition of two of them can be proved.
            Literal::Number(held) => {
                match held.fract() == 0.0 && i32::try_from(*held as i64).is_ok() {
                    true => Const::Int(*held as i64),
                    false => Const::Float(*held),
                }
            }
            Literal::Boolean(held) => Const::Bool(*held),
            Literal::Singleton(which) => Const::Declared(*which as u32),
            // A STRING is a constant of the LANGUAGE's table, not of the IR's. The
            // IR carries the index; `domain::JsConst` says it is text. Nothing here
            // reaches an interner the machine holds — that is the lowering to the
            // machine's question, and it is one layer down.
            Literal::String(text) => {
                let index = self.domain.constant(JsConst::Text(text.clone()));
                Const::Declared(index)
            }
            // NAMED APART rather than sharing a bucket. A regular expression is an
            // object the runtime builds and a bigint is a second numeric tower, and a
            // survey that counts them together says neither.
            // A REGULAR EXPRESSION is an entry point, and the first one this lowering
            // names. Compiling a pattern, allocating the object and installing its
            // state are not things a lowering can express as instructions -- and they
            // are one thing the runtime does, which is what an entry point is.
            //
            // Two evaluations of one literal are two objects, because the object
            // carries `lastIndex`. So the call is emitted where the literal is written
            // rather than hoisted to the top of the function, and a pass that wants to
            // hoist it has to prove the state is never read.
            Literal::Regex { pattern, flags } => {
                let pattern =
                    crate::syntax::Text::from_units(pattern.encode_utf16().collect::<Vec<u16>>());
                let flags =
                    crate::syntax::Text::from_units(flags.encode_utf16().collect::<Vec<u16>>());
                let pattern = self.domain.constant(JsConst::Text(pattern));
                let pattern = self.declared(pattern, at);
                let flags = self.domain.constant(JsConst::Text(flags));
                let flags = self.declared(flags, at);
                let entry = self.domain.entry_point(crate::domain::JsEntry::RegexNew);
                return Ok(self.call(
                    rts_mir::cfg::Callee::Entry(entry),
                    None,
                    vec![pattern, flags],
                    at,
                ));
            }
            _ => {
                return Err(Unsupported::Expression(
                    "a bigint literal is a second numeric tower",
                ));
            }
        };
        let of = self.domain.of_const(&value);
        let held = self.builder.push(Op::Const(value), Effect::PURE, at.at);
        self.types.insert(held, of);
        Ok(held)
    }

    pub(super) fn singleton(&mut self, which: Singleton, at: &Stmt) -> ValueId {
        let value = Const::Declared(which as u32);
        let of = self.domain.of_const(&value);
        let held = self.builder.push(Op::Const(value), Effect::PURE, at.at);
        self.types.insert(held, of);
        held
    }

    /// A singleton, at an expression's position.
    ///
    /// Beside the statement-positioned one because `void a` needs it and holds an
    /// `Expr`; keeping one function taking a `Position` would have been the third
    /// shape of the same three lines.
    pub(super) fn singleton_at(&mut self, which: Singleton, at: &Expr) -> ValueId {
        let value = Const::Declared(which as u32);
        let of = self.domain.of_const(&value);
        let held = self.builder.push(Op::Const(value), Effect::PURE, at.at);
        self.types.insert(held, of);
        held
    }

    /// Pushes a primitive, with the effect its operand types imply.
    ///
    /// This is where `domain::Js::effect_of` earns its shape: the same operator
    /// is `PURE` over two proven numbers and `CALLS_USER|THROWS` over two
    /// unknowns, so what a pass may later do with this instruction is decided
    /// here, from what is known here.
    pub(super) fn prim(&mut self, which: JsPrim, args: Vec<ValueId>, at: &Expr) -> ValueId {
        // SPECULATE FIRST, where the operation is one that coerces to a number anyway.
        // A guard's result is the same value narrowed, so what follows reads the proved
        // form and the effect below is computed from it -- which is the difference
        // between `PURE` and `CALLS_USER|THROWS` for the same operator.
        let args = match which.coerces_to_number() {
            true => self.speculate_numeric(&args, at),
            false => args,
        };
        let of_args: Vec<Type> = args.iter().map(|held| self.type_of(*held)).collect();
        let prim = self.domain.prim(which);
        let effect = self.domain.effect_of(prim, &of_args);
        let answered = self.domain.transfer(prim, &of_args);
        let held = self.builder.push(Op::Prim { prim, args }, effect, at.at);
        self.types.insert(held, answered);
        held
    }
}

impl Lowering<'_> {
    /// A declared constant, as a value.
    ///
    /// Here rather than written out at each site because three of them need one and
    /// the type has to come from the domain: a key is text, and a pass reading the
    /// graph should see that rather than a bare number.
    pub(super) fn declared(&mut self, index: u32, at: &Expr) -> ValueId {
        let value = Const::Declared(index);
        let of = self.domain.of_const(&value);
        let held = self.builder.push(Op::Const(value), Effect::PURE, at.at);
        self.types.insert(held, of);
        held
    }
}
