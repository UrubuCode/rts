//! Process-global SHAPE registry — `GlobalShapeId` → ordered key list.
//!
//! Moved here from `rts-runtime::shape` so the WHOLE runtime layer (rts-std /
//! rts-shared, which sit BELOW rts-runtime) can mint shaped objects — the
//! `{ shape_id, slots }` object model — instead of legacy `Entry::Map`
//! dictionary rows. The compile-time `ShapeTable` (per-compilation local ids)
//! stays in `rts-runtime::shape`; only the process-global id registry and the
//! Error-class registry live here.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};


pub type GlobalShapeId = u32;

/// The process-global registry mapping a [`GlobalShapeId`] → its ordered key
/// list. Populated at lowering time (object-literal/class interning) and by
/// runtime producers of shaped objects; read by the inspect / dynamic-property
/// trampolines via [`global_shape_keys`].
struct GlobalShapeRegistry {
    keys: Vec<Vec<String>>,
    by_keys: HashMap<Vec<String>, GlobalShapeId>,
    /// Key → slot index, PARALLEL to `keys` by position: `slots[i]` indexes
    /// `keys[i]`. `None` for a shape below [`SLOT_INDEX_MIN_KEYS`], where the
    /// linear scan genuinely wins (hashing a key costs more than a handful of
    /// `str` compares).
    ///
    /// An index keyed by NAME ALONE was tried before and diverged from `keys`
    /// under the parallel suite. This one cannot: it is only ever written by
    /// [`GlobalShapeRegistry::push_shape`], the single function that pushes to
    /// `keys`, so the two vectors advance together or not at all.
    slots: Vec<Option<HashMap<String, u32>>>,
    /// The TRANSITION TREE edge set, PARALLEL to `keys` by position:
    /// `transitions[i][k]` is the shape you reach by appending key `k` to shape
    /// `i`. Grown lazily — an edge appears the first time that key is added to
    /// that shape.
    ///
    /// Without it, `obj.k = v` on an absent key had to MATERIALIZE the whole key
    /// list (one `String` malloc per existing key), push, and re-hash it to
    /// intern — so building an object key by key cost O(n²).
    transitions: Vec<HashMap<String, GlobalShapeId>>,
}

/// Below this key count no index is built — the linear scan is already free
/// there, and the map would only cost memory.
///
/// MEASURED (200k dynamic reads, release, worst-case last-key lookup): at 16
/// keys index and scan are indistinguishable (85 vs 87 ms); at 256 keys the
/// index is 86 ms against 178 ms — 2.07×. The counterfactual was run by
/// building with this constant at `usize::MAX`, not assumed.
const SLOT_INDEX_MIN_KEYS: usize = 8;

impl GlobalShapeRegistry {
    /// Append one shape's key list and its matching slot index. THE only way to
    /// grow `keys` — see the `slots` doc for why that matters.
    fn push_shape(&mut self, keys: Vec<String>) {
        self.slots.push(Self::build_index(&keys));
        self.transitions.push(HashMap::new());
        self.keys.push(keys);
    }

    fn build_index(keys: &[String]) -> Option<HashMap<String, u32>> {
        if keys.len() < SLOT_INDEX_MIN_KEYS {
            return None;
        }
        let mut m = HashMap::with_capacity(keys.len());
        // FIRST occurrence wins, matching `keys.iter().position(..)` exactly —
        // a duplicated key in a shape must resolve to the same slot both ways.
        for (i, k) in keys.iter().enumerate() {
            m.entry(k.clone()).or_insert(i as u32);
        }
        Some(m)
    }
}

fn registry() -> &'static Mutex<GlobalShapeRegistry> {
    static REG: OnceLock<Mutex<GlobalShapeRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(GlobalShapeRegistry {
            keys: Vec::new(),
            by_keys: HashMap::new(),
            slots: Vec::new(),
            transitions: Vec::new(),
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
/// Checked on READ, not on write, so it also covers [`seed_global_shapes`],
/// which rebuilds `by_keys` from the positional snapshot and would otherwise
/// re-publish class rows into it on every AOT start and every cache replay.
fn is_class_owned(id: GlobalShapeId) -> bool {
    class_shapes()
        .lock()
        .map(|t| t.by_id.contains_key(&id))
        .unwrap_or(false)
}

/// Intern `keys` in the PROCESS-GLOBAL registry, returning a stable
/// [`GlobalShapeId`]. Idempotent for an identical key-sequence.
///
/// A content hit on a CLASS-owned id is treated as a MISS (see
/// [`is_class_owned`]): the caller gets a fresh layout id of its own, never the
/// class's identity.
pub fn intern_global_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    if let Some(&id) = reg.by_keys.get(keys) {
        if !is_class_owned(id) {
            return id;
        }
    }
    let id = GLOBAL_SHAPE_BASE + reg.keys.len() as GlobalShapeId;
    reg.push_shape(keys.to_vec());
    // Overwrite a class row's content entry with this layout id, so later
    // interns of the same key list hit immediately instead of re-checking.
    reg.by_keys.insert(keys.to_vec(), id);
    id
}

/// The shape reached by APPENDING `key` to shape `from` — the transition-tree
/// edge, memoized per (shape, key).
///
/// `None` when `from` was never interned; the caller must fall back to its own
/// key-list route. `key` MUST be absent from `from` (the property-write path
/// only reaches a transition for a genuinely new key); appending a duplicate is
/// not meaningful and returns `from` unchanged rather than minting a shape whose
/// key list disagrees with its slot count.
///
/// Why this exists: `obj.k = v` on an absent key used to rebuild the whole key
/// list (`global_shape_keys` → a `String` malloc per existing key), push, and
/// re-hash the vector to intern it — making key-by-key object construction
/// O(n²). The first add of a given key to a given shape still pays that; every
/// later one is a hash lookup.
///
/// The edge is only cached for a target reached through the ORDINARY content
/// route, so the "a class-owned id is never handed out by content"
/// rule ([`is_class_owned`]) is preserved exactly — this function reuses
/// `intern_global_shape`'s decision rather than re-deriving it.
pub fn shape_with_added_key(from: GlobalShapeId, key: &str) -> Option<GlobalShapeId> {
    let idx = from.checked_sub(GLOBAL_SHAPE_BASE)? as usize;
    {
        let reg = registry().lock().expect("global shape registry poisoned");
        let existing = reg.keys.get(idx)?;
        // A duplicate key is not a transition — see the doc above.
        if existing.iter().any(|k| k == key) {
            return Some(from);
        }
        if let Some(&hit) = reg.transitions.get(idx).and_then(|t| t.get(key)) {
            return Some(hit);
        }
    }
    // MISS: build the target key list and intern it through the ordinary path.
    // The lock is RELEASED first — `intern_global_shape` takes it itself, and
    // `is_class_owned` takes the class-shape mutex, so holding it here would
    // deadlock.
    let mut keys = {
        let reg = registry().lock().expect("global shape registry poisoned");
        reg.keys.get(idx)?.clone()
    };
    keys.push(key.to_string());
    let to = intern_global_shape(&keys);
    let mut reg = registry().lock().expect("global shape registry poisoned");
    if let Some(t) = reg.transitions.get_mut(idx) {
        t.insert(key.to_string(), to);
    }
    Some(to)
}

/// Mint a GLOBALLY-UNIQUE shape id for ONE user `class` declaration (NEVER
/// de-duplicated by keys — the id is a sound per-class runtime identity for
/// `instanceof` on an opaque value). The key list is still recorded for
/// inspect.
///
/// Deliberately does NOT publish into `by_keys`: that map answers "which layout
/// has these keys", and a class id is an ANSWER TO A DIFFERENT QUESTION. See
/// [`is_class_owned`].
pub fn intern_class_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    let id = GLOBAL_SHAPE_BASE + reg.keys.len() as GlobalShapeId;
    reg.push_shape(keys.to_vec());
    id
}

/// Drop every interned global shape. Called at the START of each program
/// compile (one live program per process — see `rts-runtime::adapters::state`).
pub fn reset_global_shapes() {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.clear();
    reg.keys.shrink_to_fit();
    reg.by_keys.clear();
    reg.by_keys.shrink_to_fit();
    reg.slots.clear();
    reg.slots.shrink_to_fit();
    reg.transitions.clear();
    reg.transitions.shrink_to_fit();
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
    reg.slots = snapshot.iter().map(|k| GlobalShapeRegistry::build_index(k)).collect();
    // Transition edges are a pure memo — a seeded registry starts with none and
    // re-learns them on demand, which cannot change any id it hands out.
    reg.transitions = (0..snapshot.len()).map(|_| HashMap::new()).collect();
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

/// The KEY COUNT of a global shape, or `None` if the id was never interned.
///
/// The array-vs-object discriminator ([`looks_like_object`]) only ever needed
/// this number, but reached it through [`global_shape_keys`] — which CLONES the
/// whole key vector, i.e. one `String` malloc PER FIELD, on EVERY property
/// access. Measured on 200k dynamic reads: 4 fields 178 ms → 48 fields 1030 ms,
/// growth that survived indexing the slot lookup because the clone, not the
/// scan, was paying for it.
pub fn global_shape_len(id: GlobalShapeId) -> Option<usize> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.get(idx as usize).map(|k| k.len())
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
    // Busca sob o lock, SEM clonar (que era o custo dominante).
    //
    // Para um shape LARGO (`SLOT_INDEX_MIN_KEYS`+ chaves) responde pelo índice
    // O(1) construído junto com `keys` — a varredura linear custava ~0,10 µs por
    // campo POR LEITURA (medido: 4 campos 178 ms, 48 campos 1030 ms em 200k
    // leituras). Um índice nome→slot GLOBAL já tinha sido tentado e divergiu do
    // `keys`; este não pode, porque é posicional e só cresce em `push_shape`,
    // a única função que empurra em `keys`.
    let idx = idx as usize;
    if let Some(Some(map)) = reg.slots.get(idx) {
        return map.get(key).map(|&i| i as usize);
    }
    let keys = reg.keys.get(idx)?;
    keys.iter().position(|k| k == key)
}

mod words;
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
