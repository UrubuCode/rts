//! Gives every parked-frame type a number in the RUNNING context's registry.
//!
//! # Why the host's number cannot be kept
//!
//! [`super::FrameShape::ty`] arrives from the compiler's own
//! `rts_cranelift::types::TypeRegistry` — a fresh one per compilation, counting
//! from zero. The context counts from zero too, in ITS registry, and it spends
//! the first numbers on its own layouts: `text_type` is 0 and `spill_type` is 1.
//! So the first generator or `async` body of a program was given type 0, and its
//! frame's header said "string".
//!
//! Nothing reads a frame by its type while it is alive, so the program ran. It
//! is the SWEEP that believes the header: `collect_cycle::release` takes a dead
//! type-0 cell for a string and frees the text-slab slot named by field 0 —
//! which in a frame is a resume label or a parameter, i.e. a small integer. The
//! slab entry freed that way belonged to whichever live string had been given
//! that slot, typically one of the program's first literals; the next string
//! made reused it, and from then on two cells shared one text. Measured on
//! `fetch` in a loop (every `await` parks a frame): after the first collection
//! literal 2 read `""`, and the literals a later `new Function` appended through
//! `adopt` came out as other strings — the Function's own source for `"g"`.
//!
//! Minting a fresh number here, per shape, makes the collision impossible
//! rather than unlikely, and it needs no agreement with the compiler about which
//! numbers are reserved: the only reader of the number is the header the
//! runtime itself writes in `generator_new`. The layout declared for it is all
//! `I64` because the frame's references are traced through
//! `Context::generators` (the tracer's arm for it), never through its type.

use rts_cranelift::repr::Repr;

use super::FrameShape;
use crate::entry::Context;

/// Rewrites each shape's `ty` to a number this context minted.
pub(in crate::entry) fn owned(context: &mut Context, frames: Vec<FrameShape>) -> Vec<FrameShape> {
    frames
        .into_iter()
        .map(|mut shape| {
            let fields = vec![Repr::I64; shape.slots.max(1) as usize];
            shape.ty = context.types.declare(&fields).index() as u32;
            shape
        })
        .collect()
}
