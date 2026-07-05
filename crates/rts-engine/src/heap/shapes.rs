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

/// The number of interned global shapes (leak-test probe).
pub fn global_shape_count() -> usize {
    registry().lock().map(|r| r.keys.len()).unwrap_or(0)
}

/// The ordered keys of a [`GlobalShapeId`], or `None` if the id was never
/// interned.
pub fn global_shape_keys(id: GlobalShapeId) -> Option<Vec<String>> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.get(idx as usize).cloned()
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
