//! Emitting memory access.
//!
//! A field access is not a load, and the difference is the point of this module.
//! A raw load makes its caller assert a width, an alignment and a trust level;
//! every such assertion is a decision taken at a call site, which is the disease
//! this layer exists to cure. A field access asserts nothing — the width and the
//! representation come from the layout, and the flags follow from the layout
//! being declared at all.

use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::types::machine_type;
use crate::mem::{Addressing, RegionBase, RegionBases};
use crate::repr::Repr;

/// A heap a function may read and write.
///
/// Held together because neither half is any use alone: the bases say how a
/// reference becomes an address, and the registry says what is at that address.
pub struct Heap<'a> {
    /// How a reference becomes an address in this heap.
    pub bases: &'a RegionBases,
    /// What the objects in it look like.
    pub types: &'a crate::types::TypeRegistry,
}

/// Computes an object's address from a reference.
///
/// The reference is the payload of a boxed value — an index, not an address —
/// so this is arithmetic rather than a call. That was the measured lever: doing
/// it here rather than through a runtime function was worth more than any change
/// to the value representation.
pub fn address_of(builder: &mut FunctionBuilder, reference: Value, bases: &RegionBases) -> Value {
    match bases.addressing() {
        Addressing::Single { base, stride } => {
            let base = materialize(builder, base);
            let offset = builder.ins().imul_imm(reference, i64::from(*stride));
            builder.ins().iadd(base, offset)
        }

        Addressing::Sharded {
            table,
            selector_bits,
            stride,
        } => {
            let mask = (1i64 << selector_bits) - 1;
            let region = builder.ins().band_imm(reference, mask);
            let slot = builder.ins().ushr_imm(reference, i64::from(*selector_bits));

            let table = materialize(builder, table);
            let entry = builder.ins().imul_imm(region, 8);
            let entry = builder.ins().iadd(table, entry);
            // The table of region bases is written when a region is created and
            // read on every access. Trusting it is what makes this three
            // instructions rather than a bounds check as well; a region index
            // comes from the reference's own low bits, so it cannot be out of
            // range for a table sized to the region count.
            let base = builder
                .ins()
                .load(types::I64, MemFlags::trusted(), entry, 0);

            let offset = builder.ins().imul_imm(slot, i64::from(*stride));
            builder.ins().iadd(base, offset)
        }
    }
}

/// Reads a field, at the width its layout declares.
///
/// There is no width parameter, which is the point: a raw load takes one and a
/// caller can pass the wrong one.
pub fn field_load(builder: &mut FunctionBuilder, address: Value, offset: i32, repr: Repr) -> Value {
    builder
        .ins()
        .load(machine_type(repr), field_flags(), address, offset)
}

/// Writes a field, at the width its layout declares.
pub fn field_store(builder: &mut FunctionBuilder, address: Value, offset: i32, value: Value) {
    builder.ins().store(field_flags(), value, address, offset);
}

/// The access flags a declared field is entitled to.
///
/// A field of a registered aggregate is at a computed offset in an object this
/// heap allocated, so it is aligned by construction and cannot trap. Asserting
/// that here, once, is different in kind from asserting it at a call site: here
/// it follows from the layout being declared, and there it would be a caller's
/// promise about memory it did not lay out.
fn field_flags() -> MemFlags {
    MemFlags::trusted()
}

/// Reads a base address, however this destination provides one.
fn materialize(builder: &mut FunctionBuilder, base: &RegionBase) -> Value {
    match base {
        RegionBase::Immediate(address) => builder.ins().iconst(types::I64, *address as i64),
        // A symbol has to be declared in the module before it can be read, which
        // is why a heap whose bases live in one is only usable through a module.
        // Reaching that case without one is a wiring mistake, not a condition to
        // handle: the heap and the module are configured together.
        RegionBase::Symbol(_) => unreachable!("a symbolic base needs the module path"),
    }
}
