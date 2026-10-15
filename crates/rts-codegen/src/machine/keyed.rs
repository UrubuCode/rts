//! `o[k]` with a key the program computed, through a site that remembers the last key
//! and layout it saw -- `emit/property.rs::emit_read_keyed`, whose shape this is.
//!
//! ```text
//!   guard o is a reference ── not one ──┐
//!          │                            │
//!   cached_get_keyed(o, k) ── missed ───┤
//!          │                            │
//!        hit(value)            slow: GetIndexed(o, k)
//!          │                            │
//!          └──────► join(value) ◄───────┘
//! ```
//!
//! Every `o[k]` used to be the `GetIndexed` call alone, from `generic.rs`. That call is
//! correct for every receiver and key, and it is the whole cost of a warm site: the
//! running emitter measured 36 ns for it against 3 for `o.a`. With the MIR stage taking
//! the function, `bench/analytic.ts` `computed key` read 54 ns where the running
//! emitter reads 33, for this reason and no other.
//!
//! # A key proved a number keeps the call
//!
//! An array element is not a property of any layout, so the site would miss on every
//! pass and pay a load and a compare before the same call. The running emitter measured
//! that as 16.0 ns becoming 23.7 on `keys[i & 3]`. So the choice here is made from what
//! the inference proved about the key, which is where the running emitter reads the
//! key's representation for the same purpose.

use rts_cranelift::ir::{FuncBuilder, ValueId as MachineValue};
use rts_cranelift::repr::{RefKind, Repr};
use rts_mir::cfg::ValueId;

use super::{JsMachine, machine};
use crate::domain::Type;
use crate::runtime::RuntimeOp;

impl JsMachine<'_> {
    /// Whether `o[k]` reads through a keyed site: every key but a proved number.
    pub(super) fn reads_keyed(&self, key: ValueId) -> bool {
        !matches!(self.types.of(key), Type::Int32 | Type::Double)
    }

    /// `a[i]` with a key the inference typed a number: the element read inline from the
    /// run table, and the runtime's direct entry for every case the table cannot answer.
    ///
    /// ```text
    ///   guard a is a reference ── not one ──┐
    ///          │                            │
    ///   run_read(a, i) ──── missed ─────────┤
    ///          │                            │
    ///        hit(value)           slow: IndexNumberDirect(a, i)
    ///          │                            │
    ///          └──────► join(value) ◄───────┘
    /// ```
    ///
    /// The owner is the operand and no address is held across anything: see
    /// `rts_cranelift::ir::inst::Terminator::RunRead`. A key whose machine form is not a
    /// double or an integer keeps the plain call, which coerces it as it always did.
    pub(super) fn indexed_read(
        &mut self,
        into: &mut FuncBuilder,
        object: MachineValue,
        key: MachineValue,
    ) -> Result<MachineValue, String> {
        if !matches!(into.repr_of(key), Repr::F64 | Repr::I32) {
            return self.call_runtime(into, RuntimeOp::IndexNumberDirect, &[object, key]);
        }
        let receiver = match into.repr_of(object) {
            Repr::Tagged => object,
            _ => into.widen(object),
        };
        let position = super::coerced(into, key, Repr::F64)?;
        let reference = into.create_block();
        let narrowed = into.add_block_param(reference, Repr::Ref(RefKind::Opaque));
        let slow = into.create_block();
        let join = into.create_block();
        let result = into.add_block_param(join, Repr::Tagged);
        into.guard(receiver, Repr::Ref(RefKind::Opaque), (reference, &[]), (slow, &[]))
            .map_err(machine)?;
        into.switch_to(reference);
        into.run_read(narrowed, position, (join, &[]), (slow, &[]))
            .map_err(machine)?;
        into.switch_to(slow);
        let answered = self.call_runtime(into, RuntimeOp::IndexNumberDirect, &[receiver, position])?;
        into.jump(join, &[answered]).map_err(machine)?;
        into.switch_to(join);
        Ok(result)
    }

    /// `o[k]` through the keyed site, with `GetIndexed` on every path the site does not
    /// take.
    pub(super) fn keyed_read(
        &mut self,
        into: &mut FuncBuilder,
        object: MachineValue,
        key: MachineValue,
    ) -> Result<MachineValue, String> {
        let receiver = match into.repr_of(object) {
            Repr::Tagged => object,
            _ => into.widen(object),
        };
        // TAGGED, and the verifier refuses anything else: the site recognises the next
        // key by its raw bits, so an unboxed spelling of one key would be a second key.
        let key = match into.repr_of(key) {
            Repr::Tagged => key,
            _ => into.widen(key),
        };
        let reference = into.create_block();
        let narrowed = into.add_block_param(reference, Repr::Ref(RefKind::Opaque));
        let slow = into.create_block();
        let join = into.create_block();
        let result = into.add_block_param(join, Repr::Tagged);
        into.guard(receiver, Repr::Ref(RefKind::Opaque), (reference, &[]), (slow, &[]))
            .map_err(machine)?;
        into.switch_to(reference);
        let cache = into.declare_cache();
        into.cached_get_keyed(narrowed, key, cache, (join, &[]), (slow, &[]))
            .map_err(machine)?;
        into.switch_to(slow);
        let answered = self.call_runtime(into, RuntimeOp::GetIndexed, &[receiver, key])?;
        into.jump(join, &[answered]).map_err(machine)?;
        into.switch_to(join);
        Ok(result)
    }
}
