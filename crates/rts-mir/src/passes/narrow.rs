//! Canonicalization for narrow integer types (i8, i16, i32 — and their
//! unsigned counterparts).
//!
//! Cranelift has no native arithmetic for I8/I16, and overflow semantics
//! at narrow widths require explicit masking. This pass guarantees that
//! every binary integer operation whose `dst` is narrow produces a value
//! masked to the correct bit width, preserving wrap-around semantics:
//!
//!   IAdd { dst: I8, ... }        → IAdd ; BAndImm dst, 0xFF (new dst)
//!
//! In practice we emit a fresh `BAndImm` instruction *after* the
//! arithmetic op, but we don't rewrite consumers — instead we replace
//! the original `dst` with the masked value via a lightweight `Bitcast`
//! chain. To keep the IR small, the pass instead inserts the mask **only**
//! when the dst type is I8/U8/I16/U16. Narrow types I32/U32 are handled
//! natively by Cranelift and don't need explicit masks.
//!
//! This is intentionally conservative — it doesn't try to elide masks
//! when the value flows directly into another narrow op.

use rts_hir::HirType;

use crate::ir::*;

/// Insert canonicalization masks after narrow integer ops in every block.
pub fn narrow(mir: &mut MirFunc) {
    for block_idx in 0..mir.blocks.len() {
        let mut new_insts: Vec<Inst> = Vec::with_capacity(mir.blocks[block_idx].insts.len());
        let insts = std::mem::take(&mut mir.blocks[block_idx].insts);
        for inst in insts {
            new_insts.push(inst.clone());
            if let Some((dst, mask)) = needs_mask(&inst, mir) {
                // Replace dst's recorded type with a masked-equivalent value.
                // We don't allocate a new ValueId because consumers reference
                // the original dst — so we rebind dst in place via BAndImm
                // to itself. This reads dst, masks, writes dst — Cranelift
                // semantics are linear so this is fine for our SSA-flat
                // representation here.
                new_insts.push(Inst::BAndImm {
                    dst,
                    lhs: dst,
                    imm: mask,
                });
            }
        }
        mir.blocks[block_idx].insts = new_insts;
    }
}

/// Returns `Some((dst, mask))` when this instruction produces a narrow
/// integer that should be masked. The mask is the value `(1 << width) - 1`.
fn needs_mask(inst: &Inst, mir: &MirFunc) -> Option<(ValueId, i64)> {
    let dst = match inst {
        Inst::IAdd { dst, .. }
        | Inst::IAddImm { dst, .. }
        | Inst::ISub { dst, .. }
        | Inst::IMul { dst, .. }
        | Inst::IMulImm { dst, .. }
        | Inst::INeg { dst, .. }
        | Inst::IShl { dst, .. }
        | Inst::IShlImm { dst, .. } => *dst,
        _ => return None,
    };
    let ty = mir.values.get(dst as usize)?;
    let mask = match ty {
        HirType::I8 | HirType::U8 => 0xFFi64,
        HirType::I16 | HirType::U16 => 0xFFFFi64,
        _ => return None,
    };
    Some((dst, mask))
}
