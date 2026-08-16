//! Where a parked frame lives, answered by running one.
//!
//! `docs/engine/generators.md` left exactly one thing open before any of the
//! four pieces is written: the rewrite reaches its frame by BYTE OFFSET into an
//! aggregate, and the runtime's heap hands out fixed-stride CELLS read by slot.
//! Whether those two describe the same memory is not a question code review can
//! settle — getting it wrong is not a compile error, it is a generator that runs
//! and reads the wrong words.
//!
//! So these tests allocate a frame from the runtime's own heap, run a function
//! that `resumable_form` rewrote against it, and check both sides read the same
//! words. This crate is where they can meet: it is the only one allowed to name
//! the machine and the runtime at once.

use rts_cranelift::frame::resumable_form;
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, Signature, ValueId};
use rts_cranelift::mem::{ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::Repr;
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{Placing, Visibility, place_in_memory};
use rts_cranelift::types::TypeRegistry;
use rts_core::heap::{INLINE_SLOTS, Region, STRIDE};

/// What a resumption that unwinds leaves with.
///
/// These tests never resume abruptly — they are about where a frame LIVES — so
/// the number matters only in that the rewrite requires one. It is the tag the
/// engine uses, so a reader comparing this with `run.rs` sees the same value.
const ABRUPT: rts_cranelift::unwind::Tag = rts_cranelift::unwind::Tag(1);

/// A function that is allowed to park its frame.
fn suspending(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        may_suspend: true,
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// A rewritten function reaches nothing but its own frame, so an allocation from
/// inside one is a mistake in the rewrite rather than a case to handle.
extern "C" fn unreachable_entry(_a: i64, _b: i64) -> i64 {
    unreachable!("a rewritten function allocates nothing")
}

/// The frame's resumed slot is generic, so writing it is a reference store, and
/// a reference store owes a barrier whether or not a collector reads it yet.
extern "C" fn ignored_barrier(_object: i64, _value: i64) {}

/// Nothing here resumes a frame abruptly, so nothing here reaches this.
extern "C" fn unreached_throw(_tag: i64, _value: i64) {
    unreachable!("these tests resume ordinarily")
}

/// A generator body, near the smallest that still parks: it suspends, then
/// answers a value that was live across the suspension.
fn parks_holding_its_parameter() -> Function {
    let mut func = suspending(&[Repr::I64], &[Repr::I64]);
    let held = param(&func, 0);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    b.suspend();
    b.ret(&[held]);
    func
}

/// Places a rewritten function against `region`'s addressing and hands back
/// something callable.
///
/// The handle owning the pages is leaked: dropping it unmaps the code, and the
/// address is what the caller is about to run.
fn compile_alone(
    name: &'static str,
    resumable: &rts_cranelift::frame::Resumable,
    types: &TypeRegistry,
    region: &Region,
) -> extern "C" fn(i64) -> i64 {
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(resumable.func.signature.clone());
    let id = funcs.declare_function(shape);
    let outside: Vec<(&str, *const u8)> = vec![
        (RtEntry::Alloc.symbol(), unreachable_entry as *const u8),
        (RtEntry::WriteBarrier.symbol(), ignored_barrier as *const u8),
        // Every rewritten body now has an unwinding resumption in it, and one
        // in a function with no handler of its own hands the throw to the
        // caller through this entry. Nothing here resumes that way — these
        // tests are about where a frame LIVES — but the symbol has to resolve
        // for the code to be placed at all.
        (RtEntry::Throw.symbol(), unreached_throw as *const u8),
    ];
    let bases = RegionBases::single(RegionBase::Immediate(region.base()), region.stride());

    // SAFETY: both addresses are functions in this binary, alive for the whole
    // process, with the signatures the machine declares for those entries.
    let placed = unsafe {
        place_in_memory(
            &[Placing {
                id,
                name,
                visibility: Visibility::Exported,
                body: Some(&resumable.func),
            }],
            &outside,
            &funcs,
            types,
            Some(bases),
        )
    }
    .expect("a rewritten function has nothing left that cannot be emitted");

    let address = placed.address_of(id).expect("placed with a body");
    Box::leak(Box::new(placed));
    // SAFETY: `resumable_form` states this shape — one reference in, finished?
    // out — and it is the shape that was just placed.
    unsafe { std::mem::transmute(address) }
}

#[test]
fn a_frame_that_fits_a_cell_is_addressed_identically_by_both_sides() {
    let mut types = TypeRegistry::new();
    let func = parks_holding_its_parameter();
    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);

    // The runtime's heap, exactly as `compile_for` builds it: one region, the
    // single addressing, the runtime's stride. Nothing here is a test heap —
    // that is the whole point, since the transform's own tests already pass
    // against one shaped to suit them.
    let mut region = Region::with_capacity(16);
    let frame = region
        .alloc(layout.size, resumable.layout.ty.index() as u32)
        .expect("a frame this small fits a cell");

    // What the runtime would do to start a generator: write the parameter and
    // the "not started" label, by SLOT, because that is the only way it can
    // reach a cell.
    region
        .set_field(frame, resumable.layout.param_fields[0], 7)
        .expect("the parameter has a slot");
    region
        .set_field(frame, resumable.layout.label_field, 0)
        .expect("the label has a slot");

    let run = compile_alone("frame_in_a_cell", &resumable, &types, &region);

    // The reference the region handed out, unchanged: compiled code turns it
    // into an address with the same base and stride the region reported.
    assert_eq!(run(i64::from(frame)), 0, "it parked rather than finishing");
    assert_eq!(
        region.field(frame, resumable.layout.label_field),
        Some(1),
        "compiled code wrote the label by byte offset and the runtime read it by \
         slot — if these two disagreed, a generator would run and read the wrong words"
    );

    assert_eq!(run(i64::from(frame)), 1, "entered again, it finished");
    assert_eq!(
        region.field(frame, resumable.layout.return_fields[0]),
        Some(7),
        "a value live across the suspension survived it, in the runtime's own heap"
    );
}

#[test]
fn a_word_wide_frame_puts_its_field_n_in_slot_n() {
    let mut types = TypeRegistry::new();
    let func = parks_holding_its_parameter();
    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);

    // This is what the test above depends on and does not state: a field index
    // is usable as a slot number only because every field is a machine word.
    // The registry packs by natural alignment, so a frame holding an `I32` or a
    // `Bool` spill breaks the correspondence — the runtime would then have to
    // read a byte offset rather than a slot, and that is a decision, not a bug.
    for (field, offset) in layout.field_offsets.iter().enumerate() {
        assert_eq!(
            *offset,
            8 + 8 * field as i32,
            "field {field} is not where slot {field} is"
        );
    }
}

#[test]
fn a_frame_wider_than_a_cell_lives_in_consecutive_ones() {
    let mut types = TypeRegistry::new();
    // Enough parameters that the label and the resumed slot push it past a
    // cell, DERIVED rather than written down: this said "six, and a cell holds
    // seven" and stopped being about anything the day the cell grew — the frame
    // fit, the allocation succeeded, and the test died on the assertion it was
    // not about. Whatever `INLINE_SLOTS` is, one more than it plus the two the
    // rewrite adds does not fit.
    let params = vec![Repr::I64; INLINE_SLOTS as usize + 1];
    let mut func = suspending(&params, &[Repr::I64]);
    let held = param(&func, 0);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    b.suspend();
    b.ret(&[held]);

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    assert!(
        layout.size > STRIDE,
        "this frame is meant to be the one that does not fit: {} bytes against a \
         cell's {STRIDE}",
        layout.size
    );

    let mut region = Region::with_capacity(16);
    assert_eq!(
        region.alloc(layout.size, resumable.layout.ty.index() as u32),
        None,
        "the heap refuses it rather than handing back a cell missing its last \
         fields — so an ordinary cell is not where every frame can live"
    );
    // Where it lives instead: consecutive cells, addressed by the same base and
    // stride as everything else. Nothing about the addressing changes, which is
    // why the rewritten function below is compiled exactly as the small one was.
    let frame = region
        .alloc_spanning(layout.size, resumable.layout.ty.index() as u32)
        .expect("consecutive cells");
    let slots = layout.field_offsets.len() as u32;
    region
        .set_spanning_field(frame, resumable.layout.param_fields[0], slots, 7)
        .expect("its own field");

    let run = compile_alone("frame_across_cells", &resumable, &types, &region);

    assert_eq!(run(i64::from(frame)), 0, "it parked");
    assert_eq!(
        region.spanning_field(frame, resumable.layout.label_field, slots),
        Some(1)
    );
    assert_eq!(run(i64::from(frame)), 1, "entered again, it finished");
    assert_eq!(
        region.spanning_field(frame, resumable.layout.return_fields[0], slots),
        Some(7),
        "a frame wider than a cell reads and writes the words it owns, and the \
         cells it spans are addressed by the arithmetic one cell costs"
    );
}
