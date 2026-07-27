//! Process-global SHAPE registry — `GlobalShapeId` → ordered key list.
//!
//! Moved here from `rts-adapters::shape` so the WHOLE runtime layer (rts-std /
//! rts-shared, which sit BELOW rts-adapters) can mint shaped objects — the
//! `{ shape_id, slots }` object model — instead of legacy `Entry::Map`
//! dictionary rows. The compile-time `ShapeTable` (per-compilation local ids)
//! stays in `rts-adapters::shape`; only the process-global id registry and the
//! Error-class registry live here.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::handles::{Entry, alloc_entry};
use super::poly::{POLY_BOX_BASE, POLY_TAG_SHIFT};

pub type GlobalShapeId = u32;

/// The process-global registry mapping a [`GlobalShapeId`] → its ordered key
/// list. Populated at lowering time (object-literal/class interning) and by
/// runtime producers of shaped objects; read by the inspect / dynamic-property
/// trampolines via [`global_shape_keys`].
struct GlobalShapeRegistry {
    keys: Vec<Vec<String>>,
    by_keys: HashMap<Vec<String>, GlobalShapeId>,
}

fn registry() -> &'static Mutex<GlobalShapeRegistry> {
    static REG: OnceLock<Mutex<GlobalShapeRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(GlobalShapeRegistry {
            keys: Vec::new(),
            by_keys: HashMap::new(),
        })
    })
}

/// Base offset for every global shape id. Object slot 0 stores the shape id as
/// a boxed INT32, and the DYNAMIC array-vs-object discriminator can only tell
/// an object's slot-0 shape id from an ARRAY's coincidental first element by
/// `global_shape_keys(slot0)` matching the length. Minting ids from a high
/// base (2^30) keeps small-int array elements from being misread as shape ids.
pub const GLOBAL_SHAPE_BASE: GlobalShapeId = 0x4000_0000;

/// Intern `keys` in the PROCESS-GLOBAL registry, returning a stable
/// [`GlobalShapeId`]. Idempotent for an identical key-sequence.
pub fn intern_global_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    if let Some(&id) = reg.by_keys.get(keys) {
        return id;
    }
    let id = GLOBAL_SHAPE_BASE + reg.keys.len() as GlobalShapeId;
    reg.keys.push(keys.to_vec());
    reg.by_keys.insert(keys.to_vec(), id);
    id
}

/// Mint a GLOBALLY-UNIQUE shape id for ONE user `class` declaration (NEVER
/// de-duplicated by keys — the id is a sound per-class runtime identity for
/// `instanceof` on an opaque value). The key list is still recorded for
/// inspect.
pub fn intern_class_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    let id = GLOBAL_SHAPE_BASE + reg.keys.len() as GlobalShapeId;
    reg.keys.push(keys.to_vec());
    reg.by_keys.entry(keys.to_vec()).or_insert(id);
    id
}

/// Drop every interned global shape. Called at the START of each program
/// compile (one live program per process — see `rts-adapters::state`).
pub fn reset_global_shapes() {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.clear();
    reg.keys.shrink_to_fit();
    reg.by_keys.clear();
    reg.by_keys.shrink_to_fit();
    if let Ok(mut t) = error_classes().lock() {
        t.clear();
    }
    if let Ok(mut t) = class_shapes().lock() {
        t.by_name.clear();
        t.by_id.clear();
    }
}

/// PRIMORDIAL error-class registry: `name` → (instance shape-id, flattened
/// field layout) so runtime trampolines can fabricate REAL error instances.
#[allow(clippy::type_complexity)]
fn error_classes() -> &'static Mutex<HashMap<String, (GlobalShapeId, Vec<String>)>> {
    static T: OnceLock<Mutex<HashMap<String, (GlobalShapeId, Vec<String>)>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record an Error-family class's instance shape + field layout.
pub fn register_error_class(name: &str, shape: GlobalShapeId, fields: &[String]) {
    if let Ok(mut t) = error_classes().lock() {
        t.insert(name.to_string(), (shape, fields.to_vec()));
    }
}

/// Look up a registered Error-family class (`None` when the prelude class was
/// not lowered in this process — e.g. an AOT binary).
pub fn error_class_info(name: &str) -> Option<(GlobalShapeId, Vec<String>)> {
    error_classes().lock().ok()?.get(name).cloned()
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

fn class_shapes() -> &'static Mutex<ClassShapes> {
    static T: OnceLock<Mutex<ClassShapes>> = OnceLock::new();
    T.get_or_init(|| {
        Mutex::new(ClassShapes {
            by_name: HashMap::new(),
            by_id: HashMap::new(),
        })
    })
}

/// Record one `class` declaration's name ↔ instance-shape pair. Last
/// declaration wins by name (flat name space — a duplicated class name across
/// modules revives as the LAST one registered, a documented limitation).
pub fn register_class_shape(name: &str, shape: GlobalShapeId) {
    if name.is_empty() {
        return;
    }
    if let Ok(mut t) = class_shapes().lock() {
        t.by_name.insert(name.to_string(), shape);
        t.by_id.insert(shape, name.to_string());
    }
}

/// The instance shape id of class `name` in THIS process, or `None` when no
/// such class was lowered here.
pub fn class_shape_of(name: &str) -> Option<GlobalShapeId> {
    class_shapes().lock().ok()?.by_name.get(name).copied()
}

/// The class name owning shape `id`, or `None` for a plain object-literal shape.
pub fn class_name_of_shape(id: GlobalShapeId) -> Option<String> {
    class_shapes().lock().ok()?.by_id.get(&id).cloned()
}

/// Snapshot the class-shape registry for the prelude cache. Paired with
/// [`seed_class_shapes`].
pub fn export_class_shapes() -> Vec<(String, GlobalShapeId)> {
    class_shapes()
        .lock()
        .map(|t| t.by_name.iter().map(|(n, id)| (n.clone(), *id)).collect())
        .unwrap_or_default()
}

/// Re-seed the class-shape registry from an [`export_class_shapes`] snapshot.
pub fn seed_class_shapes(snapshot: Vec<(String, GlobalShapeId)>) {
    if let Ok(mut t) = class_shapes().lock() {
        t.by_name.clear();
        t.by_id.clear();
        for (name, id) in snapshot {
            t.by_id.insert(id, name.clone());
            t.by_name.insert(name, id);
        }
    }
}

/// The number of interned global shapes (leak-test probe).
pub fn global_shape_count() -> usize {
    registry().lock().map(|r| r.keys.len()).unwrap_or(0)
}

/// Snapshot the ordered key-lists of every interned global shape, for the
/// precompiled-prelude cache (step 10). The VEC INDEX is the shape id minus
/// [`GLOBAL_SHAPE_BASE`], so re-seeding this exact vector reproduces every id
/// exactly — which is mandatory, because prelude machine code bakes shape ids as
/// immediates.
pub fn export_global_shapes() -> Vec<Vec<String>> {
    registry()
        .lock()
        .map(|r| r.keys.clone())
        .unwrap_or_default()
}

/// Re-seed the global shape registry from a [`export_global_shapes`] snapshot,
/// reproducing every id by position. MUST run on an EMPTY registry (call
/// [`reset_global_shapes`] first) so the seeded ids line up with the baked
/// immediates; panics via the assert otherwise. Rebuilds the `by_keys` dedup map
/// so later `intern_global_shape` of a prelude key returns its ORIGINAL id and a
/// new user key mints ABOVE the seeded range.
pub fn seed_global_shapes(snapshot: Vec<Vec<String>>) {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    assert!(
        reg.keys.is_empty(),
        "seed_global_shapes on a non-empty registry ({} shapes) — reset first",
        reg.keys.len()
    );
    // FIRST-wins dedup, matching `intern_class_shape`'s `by_keys.entry().or_insert`
    // and `intern_global_shape`'s "return existing" — two class shapes can share a
    // key sequence (non-deduped ids), and a later `intern_global_shape` of that key
    // must resolve to the SAME (first) id a fresh build would. A last-wins rebuild
    // would return a different id → silent divergence from the uncached path.
    let mut by_keys = HashMap::with_capacity(snapshot.len());
    for (i, k) in snapshot.iter().enumerate() {
        by_keys
            .entry(k.clone())
            .or_insert(GLOBAL_SHAPE_BASE + i as GlobalShapeId);
    }
    reg.by_keys = by_keys;
    reg.keys = snapshot;
}

/// Snapshot the Error-class registry (`name → (shape id, field layout)`) for the
/// prelude cache. Paired with [`seed_error_classes`].
pub fn export_error_classes() -> Vec<(String, GlobalShapeId, Vec<String>)> {
    error_classes()
        .lock()
        .map(|t| {
            t.iter()
                .map(|(n, (id, f))| (n.clone(), *id, f.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Re-seed the Error-class registry from an [`export_error_classes`] snapshot.
pub fn seed_error_classes(snapshot: Vec<(String, GlobalShapeId, Vec<String>)>) {
    if let Ok(mut t) = error_classes().lock() {
        t.clear();
        for (name, id, fields) in snapshot {
            t.insert(name, (id, fields));
        }
    }
}

// ---- AOT seed blob (compiler ↔ runtime, in-process for JIT, cross-process for AOT) ----
//
// The global shape registry is populated at COMPILE time in the compiler process,
// and shape ids are baked as IMMEDIATES into the emitted code (slot-0 of every
// object, the compare arms of dynamic dispatch). The JIT shares the registry with
// the run (same process), so `global_shape_keys(baked_id)` resolves. An AOT binary
// is a SEPARATE process whose registry starts EMPTY — every DYNAMIC shape read
// (`__rtsadp_obj_get` on a Tagged/`any`/catch-bound receiver, `console.log(obj)`,
// dynamic `Object.keys`) then misses and returns `undefined`. This is the same
// class of bug as the string-pool handles (fixed by making key immediates
// AOT-safe): a compile-time value baked into code that means nothing in the AOT
// process. Here the fix transfers the id→keys registry itself: the AOT `main` shim
// embeds this blob and calls `__RTS_FN_RT_SEED_SHAPES` before `__rts_startup`.
//
// Format (little-endian, length-prefixed; a PRIVATE contract — both sides are this
// module): u32 num_shapes, then per shape { u32 num_keys, per key { u32 len, bytes }};
// u32 num_errs, then per err { u32 name_len, name_bytes, u32 shape_id, u32
// num_fields, per field { u32 len, bytes } }.

fn wr_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn wr_str(out: &mut Vec<u8>, s: &str) {
    wr_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}
fn rd_u32(b: &[u8], p: &mut usize) -> u32 {
    let v = u32::from_le_bytes([b[*p], b[*p + 1], b[*p + 2], b[*p + 3]]);
    *p += 4;
    v
}
fn rd_str(b: &[u8], p: &mut usize) -> String {
    let len = rd_u32(b, p) as usize;
    let s = String::from_utf8_lossy(&b[*p..*p + len]).into_owned();
    *p += len;
    s
}

/// Serialize the current global-shape + error-class registries into a flat byte
/// blob the AOT binary re-seeds at startup ([`seed_from_blob`]). Call AFTER the
/// whole program is lowered (every shape interned). See the module note above.
pub fn export_seed_blob() -> Vec<u8> {
    let shapes = export_global_shapes();
    let errs = export_error_classes();
    let mut out = Vec::new();
    wr_u32(&mut out, shapes.len() as u32);
    for keys in &shapes {
        wr_u32(&mut out, keys.len() as u32);
        for k in keys {
            wr_str(&mut out, k);
        }
    }
    wr_u32(&mut out, errs.len() as u32);
    for (name, id, fields) in &errs {
        wr_str(&mut out, name);
        wr_u32(&mut out, *id);
        wr_u32(&mut out, fields.len() as u32);
        for f in fields {
            wr_str(&mut out, f);
        }
    }
    // Class-shape section (appended AFTER the original two — a blob written by
    // an older binary simply ends here, and `seed_from_blob` treats the missing
    // section as empty instead of misparsing).
    let classes = export_class_shapes();
    wr_u32(&mut out, classes.len() as u32);
    for (name, id) in &classes {
        wr_str(&mut out, name);
        wr_u32(&mut out, *id);
    }
    out
}

/// Re-seed both registries from an [`export_seed_blob`] blob. Resets first, so the
/// seeded ids reproduce the baked immediates exactly (`seed_global_shapes` asserts
/// an empty registry — the reset guarantees it even if something interned earlier).
/// A later runtime `intern_global_shape` (a dynamic shape transition) mints ABOVE
/// the seeded range, consistent with the compile-time numbering.
pub fn seed_from_blob(bytes: &[u8]) {
    let mut p = 0usize;
    let num_shapes = rd_u32(bytes, &mut p) as usize;
    let mut shapes: Vec<Vec<String>> = Vec::with_capacity(num_shapes);
    for _ in 0..num_shapes {
        let num_keys = rd_u32(bytes, &mut p) as usize;
        let mut keys = Vec::with_capacity(num_keys);
        for _ in 0..num_keys {
            keys.push(rd_str(bytes, &mut p));
        }
        shapes.push(keys);
    }
    let num_errs = rd_u32(bytes, &mut p) as usize;
    let mut errs: Vec<(String, GlobalShapeId, Vec<String>)> = Vec::with_capacity(num_errs);
    for _ in 0..num_errs {
        let name = rd_str(bytes, &mut p);
        let id = rd_u32(bytes, &mut p);
        let num_fields = rd_u32(bytes, &mut p) as usize;
        let mut fields = Vec::with_capacity(num_fields);
        for _ in 0..num_fields {
            fields.push(rd_str(bytes, &mut p));
        }
        errs.push((name, id, fields));
    }
    // Class-shape section — absent in a blob written before it existed.
    let mut classes: Vec<(String, GlobalShapeId)> = Vec::new();
    if p + 4 <= bytes.len() {
        let num_classes = rd_u32(bytes, &mut p) as usize;
        classes.reserve(num_classes);
        for _ in 0..num_classes {
            let name = rd_str(bytes, &mut p);
            let id = rd_u32(bytes, &mut p);
            classes.push((name, id));
        }
    }
    reset_global_shapes();
    seed_global_shapes(shapes);
    seed_error_classes(errs);
    seed_class_shapes(classes);
}

/// The ordered keys of a [`GlobalShapeId`], or `None` if the id was never
/// interned.
pub fn global_shape_keys(id: GlobalShapeId) -> Option<Vec<String>> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.get(idx as usize).cloned()
}

/// The SLOT INDEX of `key` in a global shape, resolved UNDER the lock — no clone.
///
/// `global_shape_keys` hands back an owned `Vec<String>`, so every property read
/// through the dynamic path (`__rtsadp_obj_get` → `resolve_slot`) allocated one
/// `String` PER FIELD of the class just to find one of them, on top of taking the
/// global mutex. Measured on a 5-field class: ~1.4 µs per field read, with the
/// cost growing with the class's field COUNT — 500 objects × ~10 reads blew a
/// 60 fps frame budget before any real work happened.
///
/// This compares in place and returns just the index.
pub fn global_shape_slot_of(id: GlobalShapeId, key: &str) -> Option<usize> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().lock().expect("global shape registry poisoned");
    // Busca sob o lock, SEM clonar (que era o custo dominante). Um índice
    // nome→slot chegou a ser tentado, mas divergiu do `keys` em execução
    // paralela da suíte (21 falhas vs 9) — a lista é a fonte de verdade única.
    let keys = reg.keys.get(idx as usize)?;
    keys.iter().position(|k| k == key)
}

/// INT32 tag of the PolyValue NaN-box (shape ids ride slot 0 as boxed INT32).
const POLY_TAG_INT32: u64 = 1;
/// Singleton tag + payloads (mirrors `rts-adapters::value::layout` — frozen ABI).
const POLY_TAG_SINGLETON: u64 = 2;
const SINGLETON_NULL: u64 = 1;
const SINGLETON_FALSE: u64 = 2;
const SINGLETON_TRUE: u64 = 3;
/// String / object / function tags (frozen ABI, mirrors the value model).
const POLY_TAG_STR: u64 = 3;
const POLY_TAG_OBJECT: u64 = 4;
const POLY_TAG_FUNCTION: u64 = 5;

/// The `true`/`false` singleton PolyValue words.
pub fn bool_word(b: bool) -> u64 {
    let sel = if b { SINGLETON_TRUE } else { SINGLETON_FALSE };
    POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | sel
}

/// The `null` singleton PolyValue word.
pub fn null_word() -> u64 {
    POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | SINGLETON_NULL
}

/// Allocate `bytes` as a fresh string entry and return its STR PolyValue word.
pub fn string_word(bytes: &[u8]) -> u64 {
    let h = alloc_entry(Entry::String(bytes.to_vec()));
    let slot = super::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
    POLY_BOX_BASE | (POLY_TAG_STR << POLY_TAG_SHIFT) | slot
}

/// Box the raw handle `h` as the PolyValue word matching its LIVE entry kind
/// (String → STR word, Function → FUNCTION word, anything else → OBJECT word).
/// `0` → null word. The runtime-layer mirror of the engine's
/// `__rtsadp_box_handle_auto` top-level (no element normalization).
pub fn handle_word_auto(h: u64) -> u64 {
    use super::handles::{Entry as E, with_entry};
    if h == 0 {
        return null_word();
    }
    let tag = with_entry(h, |e| match e {
        Some(E::String(_)) => POLY_TAG_STR,
        Some(E::Function(_)) => POLY_TAG_FUNCTION,
        Some(_) => POLY_TAG_OBJECT,
        None => 0,
    });
    if tag == 0 {
        return null_word();
    }
    let slot = super::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
    POLY_BOX_BASE | (tag << POLY_TAG_SHIFT) | slot
}

/// Legacy raw-i64 sentinels (the era-i64 surface: TPL_COERCE/codegen convention).
const RAW_BOOL_FALSE: i64 = i64::MIN;
const RAW_BOOL_TRUE: i64 = i64::MIN + 1;
const RAW_UNDEFINED: i64 = i64::MIN + 2;
const RAW_NULL: i64 = i64::MIN + 3;

/// Convert a LEGACY raw-i64 surface value into a PolyValue word: an
/// already-boxed word passes through; the era-i64 sentinels map to their
/// singletons; a live raw handle (≥ 2^48) boxes by entry kind; an exact-i32
/// int boxes as INT32; anything else passes through verbatim (inline-double
/// bits are already a valid word).
pub fn legacy_i64_to_word(raw: i64) -> u64 {
    let w = raw as u64;
    if (w & POLY_BOX_BASE) == POLY_BOX_BASE {
        return w;
    }
    match raw {
        RAW_BOOL_FALSE => return bool_word(false),
        RAW_BOOL_TRUE => return bool_word(true),
        RAW_UNDEFINED => return super::poly::POLY_UNDEFINED,
        RAW_NULL => return null_word(),
        _ => {}
    }
    if w >= (1 << 48) {
        use super::handles::with_entry;
        let live = with_entry(w, |e| e.is_some());
        if live {
            return handle_word_auto(w);
        }
    }
    if let Ok(i) = i32::try_from(raw) {
        return POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | (i as u32 as u64);
    }
    w
}

/// The boxed-INT32 PolyValue word carrying `id` (what object slot 0 stores).
pub fn shape_id_word(id: GlobalShapeId) -> u64 {
    POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | id as u64
}

/// Allocate a runtime SHAPED object `{ keys[i]: values[i] }` — the engine's
/// object representation (`Entry::Vec` with slot 0 = boxed shape id, then one
/// PolyValue-word slot per key). `values` are PolyValue WORDS (or raw i64s a
/// consumer normalizes — prefer words). Returns the raw handle.
pub fn alloc_shaped_object(keys: &[&str], values: &[i64]) -> u64 {
    let owned: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
    alloc_shaped_object_owned(owned, values)
}

/// [`alloc_shaped_object`] with OWNED keys (runtime-computed key lists — e.g.
/// the numbered keys of a regex match row).
pub fn alloc_shaped_object_owned(keys: Vec<String>, values: &[i64]) -> u64 {
    debug_assert_eq!(keys.len(), values.len());
    let id = intern_global_shape(&keys);
    let mut slots: Vec<i64> = Vec::with_capacity(values.len() + 1);
    slots.push(shape_id_word(id) as i64);
    slots.extend_from_slice(values);
    alloc_entry(Entry::Vec(Box::new(slots)))
}

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
