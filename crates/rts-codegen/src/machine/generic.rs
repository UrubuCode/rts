//! The generic arm of every operation: a call into the runtime, as the running engine
//! makes it.
//!
//! # Why this is not "lowering the operator"
//!
//! `README.md` rule 5: a value whose representation the language could not establish is
//! generic, and its operators are CALLS. `emit/expr.rs` does exactly this for every
//! operator over a tagged value -- `a + b` decides between concatenation and arithmetic
//! from its operands at run time, so it is the runtime's `Add` that is called, never an
//! instruction chosen on the strength of the operands looking alike.
//!
//! So nothing here decides a semantic. Each row names the `RuntimeOp` the running engine
//! calls for the same operator, and the runtime's implementation is the one definition of
//! it (rule 3). What IS decided here is which rows may have one at all, and the reason a
//! row is absent is always stated where the row would be.
//!
//! # What the answer is, and why it cannot lie
//!
//! A call answers what its signature declares -- a boolean for a comparison, a tagged
//! word otherwise. Where the lattice promised more than a tagged word (a numeric row over
//! one operand that rules a BigInt out answers `Double`), the machine value is still the
//! tagged one, and every consumer that needs the double refuses rather than reading one:
//! `as_double` refuses a tagged operand and a coercion from tagged is a narrowing. So a
//! promise the call cannot keep costs a refusal downstream, never a wrong answer.

use rts_cranelift::ir::{FuncBuilder, ValueId as MachineValue};

use super::JsMachine;
use crate::domain::JsPrim;
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};

impl JsMachine<'_> {
    /// The runtime's answer to `which` over these operands, or `None` where the row
    /// has no generic form here.
    pub(super) fn generic(
        &mut self,
        into: &mut FuncBuilder,
        which: JsPrim,
        args: &[MachineValue],
    ) -> Result<Option<MachineValue>, String> {
        let op = match (which, args.len()) {
            (JsPrim::Add, 2) => RuntimeOp::Add,
            (JsPrim::Subtract, 2) => RuntimeOp::Subtract,
            (JsPrim::Multiply, 2) => RuntimeOp::Multiply,
            (JsPrim::Divide, 2) => RuntimeOp::Divide,
            (JsPrim::Remainder, 2) => RuntimeOp::Remainder,
            (JsPrim::LessThan, 2) => RuntimeOp::Less,
            (JsPrim::GreaterThan, 2) => RuntimeOp::Greater,
            (JsPrim::LessOrEqual, 2) => RuntimeOp::LessEqual,
            (JsPrim::GreaterOrEqual, 2) => RuntimeOp::GreaterEqual,
            (JsPrim::StrictEquals, 2) => RuntimeOp::StrictEquals,
            // THE CHEAP ARMS FIRST is the runtime's to keep, and it does:
            // `docs/codegen/entry-tax.md` part five is the finding that made it so.
            (JsPrim::LooseEquals, 2) => RuntimeOp::LooseEquals,
            (JsPrim::InstanceOf, 2) => RuntimeOp::InstanceOf,
            (JsPrim::HasProperty, 2) => RuntimeOp::HasProperty,
            (JsPrim::IndexRead, 2) => RuntimeOp::GetIndexed,
            (JsPrim::Truthy, 1) => RuntimeOp::ToBoolean,
            (JsPrim::TypeOf, 1) => RuntimeOp::TypeOf,
            (JsPrim::Negate, 1) => RuntimeOp::Negate,
            (JsPrim::BitwiseNot, 1) => RuntimeOp::BitNot,
            // A WRITE answers the value written, which the graph types as the value.
            (JsPrim::IndexWrite, 3) => {
                let strict = self.word(into, 0);
                self.call_runtime(
                    into,
                    RuntimeOp::SetIndexed,
                    &[args[0], args[1], args[2], strict],
                )?;
                return Ok(Some(args[2]));
            }
            (JsPrim::FieldWrite, 3) => {
                let key = self.key_from_operand(args[1])?;
                self.store_through_cache(into, args[0], key, args[1], args[2], false)?;
                return Ok(Some(args[2]));
            }
            (JsPrim::Construct, count) if count >= 1 => {
                return self.construct(into, args).map(Some);
            }
            (JsPrim::NewArray, _) => return self.array_of(into, args).map(Some),
            (JsPrim::NewObject, _) => return self.object_of(into, args).map(Some),
            // NOT HERE, each for a reason of its own:
            //
            // - `BitwiseInt32` holds five operators under one row, so WHICH runtime
            //   operation is not in the graph -- the fault `Compare` had until it was
            //   split into its three rows;
            // - `ToNumber` is used for `i++`, which applies ToNumeric -- a BigInt
            //   increments to a BigInt -- while `UnaryPlus` throws on one, so the call
            //   that exists answers a different operation;
            // - `Not` and `IsNullish` have no call of their own in the catalogue.
            _ => return Ok(None),
        };
        self.call_runtime(into, op, args).map(Some)
    }

    /// `new f(a, b)`: the constructor, then one slot per argument padded with
    /// `undefined` -- the arity `RuntimeOp::Construct` fixes, and the same refusal
    /// past it that a call has.
    fn construct(
        &mut self,
        into: &mut FuncBuilder,
        args: &[MachineValue],
    ) -> Result<MachineValue, String> {
        let written = args.len() - 1;
        if written > ARGUMENT_SLOTS {
            return Err(format!(
                "a construction of {written} arguments needs the vector form, and this door has {ARGUMENT_SLOTS} slots"
            ));
        }
        let undefined = self.undefined(into)?;
        let mut of_args = args.to_vec();
        of_args.resize(1 + ARGUMENT_SLOTS, undefined);
        self.call_runtime(into, RuntimeOp::Construct, &of_args)
    }

    /// An array literal of at most four elements, through `ArrayOf` -- the count
    /// written, then the elements padded with `undefined`. The count is what keeps the
    /// padding out of the array.
    fn array_of(
        &mut self,
        into: &mut FuncBuilder,
        args: &[MachineValue],
    ) -> Result<MachineValue, String> {
        if args.len() > ARGUMENT_SLOTS {
            return Err(format!(
                "an array literal of {} elements needs the appending form, and ArrayOf has {ARGUMENT_SLOTS} slots",
                args.len()
            ));
        }
        let count = self.word(into, args.len() as u64);
        let undefined = self.undefined(into)?;
        let mut of_args = vec![count];
        of_args.extend_from_slice(args);
        of_args.resize(1 + ARGUMENT_SLOTS, undefined);
        self.call_runtime(into, RuntimeOp::ArrayOf, &of_args)
    }

    /// An object literal: an empty object as wide as the literal, then every pair
    /// DEFINED in source order -- `emit/object.rs` defines rather than assigns, and the
    /// order is what decides the layout.
    ///
    /// `__proto__: v`, which SETS the prototype, never arrives here: the tree holds it as
    /// `Property::Prototype`, which the lowering refuses. What can arrive under that
    /// spelling -- `{ __proto__ }` shorthand, a quoted key -- is an ordinary own property,
    /// which is exactly what a define makes.
    fn object_of(
        &mut self,
        into: &mut FuncBuilder,
        args: &[MachineValue],
    ) -> Result<MachineValue, String> {
        if args.len() % 2 != 0 {
            return Err("an object literal is pairs of a key and a value".to_owned());
        }
        let width = self.word(into, (args.len() / 2) as u64);
        let built = self.call_runtime(into, RuntimeOp::ObjectNew, &[width])?;
        for pair in args.chunks(2) {
            let key = self.key_from_operand(pair[0])?;
            self.define_through_cache(into, built, key, pair[0], pair[1])?;
        }
        Ok(built)
    }
}
