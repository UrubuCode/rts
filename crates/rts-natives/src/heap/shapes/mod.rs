//! Process-global SHAPE registry — `GlobalShapeId` → ordered key list.
//!
//! Moved here from `rts-runtime::shape` so the WHOLE runtime layer (rts-std /
//! rts-shared, which sit BELOW rts-runtime) can mint shaped objects — the
//! `{ shape_id, slots }` object model — instead of legacy `Entry::Map`
//! dictionary rows. The compile-time `ShapeTable` (per-compilation local ids)
//! stays in `rts-runtime::shape`; only the process-global id registry and the
//! Error-class registry live here.
//!
//! Split into submodules (the file passed the 500-line ceiling when the
//! `Mutex` → `RwLock` work landed):
//!
//! - [`registry`] — the STRUCTURAL registry (id ↔ key list, slot index,
//!   transition memo) and the interning rules. Carries the synchronization
//!   design and the binding LOCK ORDER note.
//! - [`classes`] — the NOMINAL registries (Error classes, class name ↔ shape).
//! - [`blob`] — the AOT seed blob both of the above serialize into.
//! - [`words`] — shaped-object allocation + PolyValue word helpers.

pub type GlobalShapeId = u32;

mod blob;
mod classes;
mod registry;
mod words;

pub use blob::{export_seed_blob, seed_from_blob};
pub use classes::{
    class_name_of_shape, class_shape_of, error_class_info, export_class_shapes,
    export_error_classes, register_class_shape, register_error_class, seed_class_shapes,
    seed_error_classes,
};
pub use registry::{
    GLOBAL_SHAPE_BASE, export_global_shapes, global_shape_count, global_shape_keys,
    global_shape_len, global_shape_slot_of, intern_class_shape, intern_global_shape,
    reset_global_shapes, seed_global_shapes, shape_with_added_key,
};
pub use words::{
    alloc_shaped_object, alloc_shaped_object_owned, alloc_shaped_object_with_id, bool_word,
    f64_word, handle_word_auto, int_word, legacy_i64_to_word, null_word, shape_id_word,
    string_word,
};

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn shape_snapshot_round_trips_ids() {
        // Intern a few shapes, snapshot, reset, re-seed → the SAME keys must map
        // to the SAME ids (the invariant the baked-immediate prelude relies on).
        reset_global_shapes();
        let a = intern_global_shape(&["x".into(), "y".into()]);
        let b = intern_global_shape(&["z".into()]);
        let snap = export_global_shapes();
        assert_eq!(global_shape_count(), 2);

        reset_global_shapes();
        assert_eq!(global_shape_count(), 0);
        seed_global_shapes(snap);

        // Same key sequences resolve to the ORIGINAL ids (dedup map rebuilt).
        assert_eq!(intern_global_shape(&["x".into(), "y".into()]), a);
        assert_eq!(intern_global_shape(&["z".into()]), b);
        // A NEW key mints ABOVE the seeded range.
        let c = intern_global_shape(&["w".into()]);
        assert_eq!(c, GLOBAL_SHAPE_BASE + 2);
        assert_eq!(global_shape_keys(a), Some(vec!["x".into(), "y".into()]));

        reset_global_shapes();
    }
}
