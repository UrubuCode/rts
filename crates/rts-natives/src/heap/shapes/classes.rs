//! The two NOMINAL registries that sit BESIDE the structural shape registry:
//! the PRIMORDIAL Error-class table and the generic `class name ↔ shape id`
//! table.
//!
//! Both are `RwLock`, not `Mutex`, for the same reason the shape registry is
//! (see `registry.rs`): every hot consumer here is a READ
//! (`class_name_of_shape` on every inspect of an object, `error_class_info` on
//! every thrown error, `is_class_owned` on the intern path), while writes only
//! happen while a program is being lowered or re-seeded.
//!
//! LOCK ORDER (binding): these locks are always the INNER ones —
//! `SHAPE_REGISTRY` → `CLASS_SHAPES` / `ERROR_CLASSES`. Nothing in this file
//! touches the shape registry, so the order cannot invert.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use super::GlobalShapeId;

/// PRIMORDIAL error-class registry: `name` → (instance shape-id, flattened
/// field layout) so runtime trampolines can fabricate REAL error instances.
#[allow(clippy::type_complexity)]
fn error_classes() -> &'static RwLock<HashMap<String, (GlobalShapeId, Vec<String>)>> {
    static T: OnceLock<RwLock<HashMap<String, (GlobalShapeId, Vec<String>)>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Record an Error-family class's instance shape + field layout.
pub fn register_error_class(name: &str, shape: GlobalShapeId, fields: &[String]) {
    if let Ok(mut t) = error_classes().write() {
        t.insert(name.to_string(), (shape, fields.to_vec()));
    }
}

/// Look up a registered Error-family class (`None` when the prelude class was
/// not lowered in this process — e.g. an AOT binary).
pub fn error_class_info(name: &str) -> Option<(GlobalShapeId, Vec<String>)> {
    error_classes().read().ok()?.get(name).cloned()
}

/// Snapshot the Error-class registry (`name → (shape id, field layout)`) for the
/// prelude cache. Paired with [`seed_error_classes`].
pub fn export_error_classes() -> Vec<(String, GlobalShapeId, Vec<String>)> {
    error_classes()
        .read()
        .map(|t| {
            t.iter()
                .map(|(n, (id, f))| (n.clone(), *id, f.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Re-seed the Error-class registry from an [`export_error_classes`] snapshot.
pub fn seed_error_classes(snapshot: Vec<(String, GlobalShapeId, Vec<String>)>) {
    if let Ok(mut t) = error_classes().write() {
        t.clear();
        for (name, id, fields) in snapshot {
            t.insert(name, (id, fields));
        }
    }
}

/// Clear the Error-class table. Called only from `reset_global_shapes`, which
/// already holds the shape-registry write lock — see the lock-order note above.
pub(super) fn clear_error_classes() {
    if let Ok(mut t) = error_classes().write() {
        t.clear();
    }
}

/// GENERIC class-shape registry: `class name ↔ instance shape id`, for every
/// user/prelude `class` declaration (the Error-family table above predates this
/// and stays — it also carries the field layout error trampolines need). The
/// consumer is the pickle (`heap::pickle`): the encoder asks "which class is
/// this shape?" ([`class_name_of_shape`]) and the decoder asks "which shape does
/// this class have HERE?" ([`class_shape_of`]) — reviving with the DESTINATION
/// program's shape id is what makes `instanceof` (a baked shape-id compare)
/// work on a revived instance.
struct ClassShapes {
    by_name: HashMap<String, GlobalShapeId>,
    by_id: HashMap<GlobalShapeId, String>,
}

fn class_shapes() -> &'static RwLock<ClassShapes> {
    static T: OnceLock<RwLock<ClassShapes>> = OnceLock::new();
    T.get_or_init(|| {
        RwLock::new(ClassShapes {
            by_name: HashMap::new(),
            by_id: HashMap::new(),
        })
    })
}

/// A CLASS shape id must never be reachable by CONTENT lookup. A shape id does
/// two different jobs — structural LAYOUT (which slot holds which key, which
/// WANTS dedup so two `{x, y}` literals share) and nominal IDENTITY (which class
/// this is, which FORBIDS dedup). `by_keys` is the layout map; handing a class id
/// out of it lets a plain object inherit a class's identity, and since
/// `class/vdispatch.rs` dispatches on a flat compare of the slot-0 shape word
/// with no constructor check, that object then EXECUTES the class's methods.
/// Reproduced before this guard existed, on every content route — object
/// literal, dynamic key adds, `{...instance}`, `Object.assign`, and
/// `JSON.parse` (i.e. external data acquiring a class identity).
///
/// Checked on READ, not on write, so it also covers `seed_global_shapes`,
/// which rebuilds `by_keys` from the positional snapshot and would otherwise
/// re-publish class rows into it on every AOT start and every cache replay.
///
/// This is the INNER lock of the one nested pair in this module tree
/// (`intern_global_shape` calls it while holding the shape registry). It takes
/// only the class table and never calls back into the registry, so the pair
/// cannot invert.
pub(super) fn is_class_owned(id: GlobalShapeId) -> bool {
    class_shapes()
        .read()
        .map(|t| t.by_id.contains_key(&id))
        .unwrap_or(false)
}

/// Record one `class` declaration's name ↔ instance-shape pair. Last
/// declaration wins by name (flat name space — a duplicated class name across
/// modules revives as the LAST one registered, a documented limitation).
pub fn register_class_shape(name: &str, shape: GlobalShapeId) {
    if name.is_empty() {
        return;
    }
    if let Ok(mut t) = class_shapes().write() {
        t.by_name.insert(name.to_string(), shape);
        t.by_id.insert(shape, name.to_string());
    }
}

/// The instance shape id of class `name` in THIS process, or `None` when no
/// such class was lowered here.
pub fn class_shape_of(name: &str) -> Option<GlobalShapeId> {
    class_shapes().read().ok()?.by_name.get(name).copied()
}

/// The class name owning shape `id`, or `None` for a plain object-literal shape.
pub fn class_name_of_shape(id: GlobalShapeId) -> Option<String> {
    class_shapes().read().ok()?.by_id.get(&id).cloned()
}

/// Snapshot the class-shape registry for the prelude cache. Paired with
/// [`seed_class_shapes`].
pub fn export_class_shapes() -> Vec<(String, GlobalShapeId)> {
    class_shapes()
        .read()
        .map(|t| t.by_name.iter().map(|(n, id)| (n.clone(), *id)).collect())
        .unwrap_or_default()
}

/// Re-seed the class-shape registry from an [`export_class_shapes`] snapshot.
pub fn seed_class_shapes(snapshot: Vec<(String, GlobalShapeId)>) {
    if let Ok(mut t) = class_shapes().write() {
        t.by_name.clear();
        t.by_id.clear();
        for (name, id) in snapshot {
            t.by_id.insert(id, name.clone());
            t.by_name.insert(name, id);
        }
    }
}

/// Clear the class-shape table. Called only from `reset_global_shapes` — see
/// the lock-order note above.
pub(super) fn clear_class_shapes() {
    if let Ok(mut t) = class_shapes().write() {
        t.by_name.clear();
        t.by_id.clear();
    }
}
