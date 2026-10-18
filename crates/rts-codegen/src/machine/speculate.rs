//! The three rows `optimize::version_loops` emits: a number test, a property peek, and the
//! double either proved.
//!
//! ```text
//!   guard v is a double ── not ──▶ guard v is an i32 ── not ──▶ (false, 0.0) ─┐
//!        │                              │                                       │
//!   (true, v) ───────────────────── (true, v as f64) ──────────────────────────┤
//!                                                                      join(answer, payload)
//! ```
//!
//! The join carries the unboxed double beside the answer, so the `NumberOf` that follows the
//! branch takes it from there and does not test the same bits a second time. It is found by
//! the TEST's value, which the graph makes an operand of `NumberOf`: two tests of one value in
//! two places cannot be confused, because each has its own.

use rts_cranelift::ir::{BlockId, ConstDecl, FuncBuilder, ScalarBits, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_mir::cfg::ValueId;

use super::{JsMachine, machine};

impl JsMachine<'_> {
    /// Ends the current block by testing a tagged `held` for being a number, jumping to
    /// `join(answer, payload)` from every path.
    fn number_split(
        &mut self,
        into: &mut FuncBuilder,
        held: MachineValue,
        join: BlockId,
    ) -> Result<(), String> {
        let double = into.create_block();
        let as_double = into.add_block_param(double, Repr::F64);
        let not_double = into.create_block();
        into.guard(held, Repr::F64, (double, &[]), (not_double, &[]))
            .map_err(machine)?;

        into.switch_to(double);
        let yes = into.bool_constant(true);
        into.jump(join, &[yes, as_double]).map_err(machine)?;

        // A number has TWO encodings, and both are numbers: an integer under its own tag is
        // read as one, exactly as `unbox_number` reads it.
        into.switch_to(not_double);
        let integer = into.create_block();
        let as_integer = into.add_block_param(integer, Repr::I32);
        let neither = into.create_block();
        into.guard(held, Repr::I32, (integer, &[]), (neither, &[]))
            .map_err(machine)?;
        into.switch_to(integer);
        let widened = into.to_f64(as_integer).map_err(machine)?;
        let yes = into.bool_constant(true);
        into.jump(join, &[yes, widened]).map_err(machine)?;

        into.switch_to(neither);
        self.not_a_number(into, join)
    }

    /// Ends the current block with the answer "no".
    fn not_a_number(&mut self, into: &mut FuncBuilder, join: BlockId) -> Result<(), String> {
        let no = into.bool_constant(false);
        let zero = into.declare_const(ConstDecl::Scalar {
            repr: Repr::F64,
            bits: ScalarBits(0),
        });
        let zero = into.use_const(zero);
        into.jump(join, &[no, zero]).map_err(machine)
    }

    fn number_join(&mut self, into: &mut FuncBuilder) -> (BlockId, MachineValue, MachineValue) {
        let join = into.create_block();
        let answer = into.add_block_param(join, Repr::Bool);
        let payload = into.add_block_param(join, Repr::F64);
        (join, answer, payload)
    }

    /// Whether `held` is a number, as a machine boolean.
    pub(super) fn number_test(
        &mut self,
        into: &mut FuncBuilder,
        test: ValueId,
        held: MachineValue,
    ) -> Result<MachineValue, String> {
        match into.repr_of(held) {
            // ALREADY A NUMBER by representation: the answer is a constant, and the payload
            // is the value in the double domain.
            Repr::F64 | Repr::I32 => {
                let double = self.as_double(into, held)?;
                self.number_payload.insert(test, double);
                Ok(into.bool_constant(true))
            }
            Repr::Tagged => {
                let (join, answer, payload) = self.number_join(into);
                self.number_split(into, held, join)?;
                into.switch_to(join);
                self.number_payload.insert(test, payload);
                Ok(answer)
            }
            // Anything else is not a number, whatever its value.
            _ => Ok(into.bool_constant(false)),
        }
    }

    /// Whether `object.key` is now an own data property holding a number -- by the cache's
    /// own hit, with no call to the runtime's read, so nothing a getter could do happens.
    pub(super) fn number_peek(
        &mut self,
        into: &mut FuncBuilder,
        test: ValueId,
        object: MachineValue,
        key_operand: MachineValue,
    ) -> Result<MachineValue, String> {
        let key = self.key_from_operand(key_operand)?;
        let receiver = match into.repr_of(object) {
            Repr::Tagged => object,
            _ => into.widen(object),
        };
        let (join, answer, payload) = self.number_join(into);
        let reference = into.create_block();
        let narrowed =
            into.add_block_param(reference, Repr::Ref(rts_cranelift::repr::RefKind::Opaque));
        let absent = into.create_block();
        let found_block = into.create_block();
        let found = into.add_block_param(found_block, Repr::Tagged);
        into.guard(
            receiver,
            Repr::Ref(rts_cranelift::repr::RefKind::Opaque),
            (reference, &[]),
            (absent, &[]),
        )
        .map_err(machine)?;

        into.switch_to(reference);
        let cache = into.declare_cache();
        // THE HIT PREPENDS THE VALUE to the block's own parameters, as `read_through_cache`
        // relies on; a miss is not a slow read here, it is "not known to be a number".
        into.cached_get(narrowed, key, cache, (found_block, &[]), (absent, &[]))
            .map_err(machine)?;

        into.switch_to(found_block);
        self.number_split(into, found, join)?;

        into.switch_to(absent);
        self.not_a_number(into, join)?;

        into.switch_to(join);
        self.number_payload.insert(test, payload);
        Ok(answer)
    }

    /// The double a passed test proved, or the checked unboxing where this does not know it.
    pub(super) fn number_of(
        &mut self,
        into: &mut FuncBuilder,
        test: ValueId,
        held: MachineValue,
    ) -> Result<MachineValue, String> {
        match self.number_payload.get(&test) {
            Some(double) => Ok(*double),
            // NOT FOUND is a graph this boundary was not told about, and the answer is still
            // right where the operand is the number: `unbox_number` guards and traps on a
            // non-number rather than reading it.
            None => self.as_double(into, held),
        }
    }
}
