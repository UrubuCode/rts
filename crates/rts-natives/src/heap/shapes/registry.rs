//! The process-global STRUCTURAL shape registry: `GlobalShapeId` → ordered key
//! list, plus the transition-tree edge memo and the slot index.
//!
//! # Why `RwLock` and not `Mutex`
//!
//! A shape is APPEND-ONLY and IMMUTABLE ONCE PUBLISHED — the same invariant V8
//! relies on for its Map graph. `keys[i]`, `slots[i]` and an id's meaning are
//! never mutated after `push_shape`; the only mutations are (a) appending a new
//! row, (b) inserting a `by_keys` / `transitions` memo entry, and (c) a whole-
//! registry `reset`/`seed` between programs. So every READER can share the lock,
//! and only interning needs exclusion. Before this change ONE process-wide
//! `Mutex` serialized every thread's every dynamic property resolution
//! (`global_shape_slot_of` is on the `__rtsadp_obj_get` path) — the item is
//! `RTS_OPTIMIZATION.md` §5 Tier 3.4 / `rts-threading-model.md` blocker #4.
//! TODO(measure): no RTS number has been taken for the contention this removes.
//!
//! # Lock order (binding)
//!
//! `SHAPE_REGISTRY` (outer) → `CLASS_SHAPES` (inner). Exactly one nesting
//! exists: `intern_global_shape` consults [`is_class_owned`] while holding the
//! registry. It cannot invert — `classes.rs` never touches this module's lock —
//! and it cannot re-enter, because `is_class_owned` calls nothing back here.
//! Every other path that would need BOTH (`shape_with_added_key`'s miss route)
//! DROPS its registry guard first: `std::sync::RwLock` is not reentrant, and a
//! second `read()` on a thread already holding one can deadlock outright once a
//! writer is queued (writer-preferring implementations park the new reader).

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use super::GlobalShapeId;
use super::classes::{clear_class_shapes, clear_error_classes, is_class_owned};

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

fn registry() -> &'static RwLock<GlobalShapeRegistry> {
    static REG: OnceLock<RwLock<GlobalShapeRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        RwLock::new(GlobalShapeRegistry {
            keys: Vec::new(),
            by_keys: HashMap::new(),
            slots: Vec::new(),
            transitions: Vec::new(),
        })
    })
}

const POISON: &str = "global shape registry poisoned";

/// Base offset for every global shape id. Object slot 0 stores the shape id as
/// a boxed INT32, and the DYNAMIC array-vs-object discriminator can only tell
/// an object's slot-0 shape id from an ARRAY's coincidental first element by
/// `global_shape_keys(slot0)` matching the length. Minting ids from a high
/// base (2^30) keeps small-int array elements from being misread as shape ids.
pub const GLOBAL_SHAPE_BASE: GlobalShapeId = 0x4000_0000;

/// Intern `keys` in the PROCESS-GLOBAL registry, returning a stable
/// [`GlobalShapeId`]. Idempotent for an identical key-sequence.
///
/// A content hit on a CLASS-owned id is treated as a MISS (see
/// `classes::is_class_owned`): the caller gets a fresh layout id of its own,
/// never the class's identity.
///
/// The id is POSITIONAL (`GLOBAL_SHAPE_BASE + keys.len()`) and that is load
/// bearing — `seed_global_shapes` rebuilds the whole table from a positional
/// snapshot, and lowered code bakes ids as immediates. The exclusive lock is
/// therefore taken for the WHOLE mint (read the length, push, publish), so two
/// threads can never observe the same length and mint the same id twice.
pub fn intern_global_shape(keys: &[String]) -> GlobalShapeId {
    // FAST PATH — SHARED lock. This is the overwhelmingly common outcome
    // (`intern_global_shape(&[])` alone runs on a dozen runtime paths).
    let hit = registry().read().expect(POISON).by_keys.get(keys).copied();
    if let Some(id) = hit {
        // The read guard is already dropped (the temporary ended with the
        // statement above) BEFORE the class lock is taken — see the lock-order
        // note. Even though registry→class is the sanctioned order, not holding
        // it here keeps the shared lock off the class table entirely.
        if !is_class_owned(id) {
            return id;
        }
    }
    let mut reg = registry().write().expect(POISON);
    // RE-CHECK under the exclusive lock. Two threads can race through the fast
    // path with the SAME key list; the loser must find the winner's row here and
    // return ITS id, so simultaneous interning yields ONE id, never two.
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
/// later one is a hash lookup — now under a SHARED lock, so concurrent readers
/// of the memo do not serialize.
///
/// The edge is only cached for a target reached through the ORDINARY content
/// route, so the "a class-owned id is never handed out by content" rule is
/// preserved exactly — this function reuses `intern_global_shape`'s decision
/// rather than re-deriving it.
pub fn shape_with_added_key(from: GlobalShapeId, key: &str) -> Option<GlobalShapeId> {
    let idx = from.checked_sub(GLOBAL_SHAPE_BASE)? as usize;
    // FAST PATH — SHARED lock: duplicate check + memo hit, no clone, no mint.
    let mut keys = {
        let reg = registry().read().expect(POISON);
        let existing = reg.keys.get(idx)?;
        // A duplicate key is not a transition — see the doc above.
        if existing.iter().any(|k| k == key) {
            return Some(from);
        }
        if let Some(&hit) = reg.transitions.get(idx).and_then(|t| t.get(key)) {
            return Some(hit);
        }
        existing.clone()
    };
    // MISS: the guard above is DROPPED here on purpose. `intern_global_shape`
    // takes the registry lock itself (and then the class lock), and
    // `std::sync::RwLock` is not reentrant — holding even a read guard across
    // that call is a deadlock, not merely a slowdown.
    keys.push(key.to_string());
    let to = intern_global_shape(&keys);
    let mut reg = registry().write().expect(POISON);
    if let Some(t) = reg.transitions.get_mut(idx) {
        // A racing thread may have memoized the same edge already; it computed
        // the same `to` (interning is idempotent), so overwriting is a no-op.
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
/// has these keys", and a class id is an ANSWER TO A DIFFERENT QUESTION.
pub fn intern_class_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().write().expect(POISON);
    let id = GLOBAL_SHAPE_BASE + reg.keys.len() as GlobalShapeId;
    reg.push_shape(keys.to_vec());
    id
}

/// Drop every interned global shape. Called at the START of each program
/// compile (one live program per process — see `rts-runtime::adapters::state`).
///
/// Holds the registry write lock across the class-table clears, which is the
/// sanctioned outer→inner order (registry → classes).
pub fn reset_global_shapes() {
    let mut reg = registry().write().expect(POISON);
    reg.keys.clear();
    reg.keys.shrink_to_fit();
    reg.by_keys.clear();
    reg.by_keys.shrink_to_fit();
    reg.slots.clear();
    reg.slots.shrink_to_fit();
    reg.transitions.clear();
    reg.transitions.shrink_to_fit();
    clear_error_classes();
    clear_class_shapes();
}

/// The number of interned global shapes (leak-test probe).
pub fn global_shape_count() -> usize {
    registry().read().map(|r| r.keys.len()).unwrap_or(0)
}

/// Snapshot the ordered key-lists of every interned global shape, for the
/// precompiled-prelude cache (step 10). The VEC INDEX is the shape id minus
/// [`GLOBAL_SHAPE_BASE`], so re-seeding this exact vector reproduces every id
/// exactly — which is mandatory, because prelude machine code bakes shape ids as
/// immediates.
pub fn export_global_shapes() -> Vec<Vec<String>> {
    registry().read().map(|r| r.keys.clone()).unwrap_or_default()
}

/// Re-seed the global shape registry from a [`export_global_shapes`] snapshot,
/// reproducing every id by position. MUST run on an EMPTY registry (call
/// [`reset_global_shapes`] first) so the seeded ids line up with the baked
/// immediates; panics via the assert otherwise. Rebuilds the `by_keys` dedup map
/// so later `intern_global_shape` of a prelude key returns its ORIGINAL id and a
/// new user key mints ABOVE the seeded range.
pub fn seed_global_shapes(snapshot: Vec<Vec<String>>) {
    let mut reg = registry().write().expect(POISON);
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
    reg.slots = snapshot
        .iter()
        .map(|k| GlobalShapeRegistry::build_index(k))
        .collect();
    // Transition edges are a pure memo — a seeded registry starts with none and
    // re-learns them on demand, which cannot change any id it hands out.
    reg.transitions = (0..snapshot.len()).map(|_| HashMap::new()).collect();
    reg.keys = snapshot;
}

/// The ordered keys of a [`GlobalShapeId`], or `None` if the id was never
/// interned.
pub fn global_shape_keys(id: GlobalShapeId) -> Option<Vec<String>> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().read().expect(POISON);
    reg.keys.get(idx as usize).cloned()
}

/// The KEY COUNT of a global shape, or `None` if the id was never interned.
///
/// The array-vs-object discriminator (`looks_like_object`) only ever needed
/// this number, but reached it through [`global_shape_keys`] — which CLONES the
/// whole key vector, i.e. one `String` malloc PER FIELD, on EVERY property
/// access. Measured on 200k dynamic reads: 4 fields 178 ms → 48 fields 1030 ms,
/// growth that survived indexing the slot lookup because the clone, not the
/// scan, was paying for it.
pub fn global_shape_len(id: GlobalShapeId) -> Option<usize> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().read().expect(POISON);
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
/// This compares in place and returns just the index, now under a SHARED lock —
/// this is THE hot reader Tier 3.4 exists for: it runs on every dynamic property
/// resolution on every thread.
pub fn global_shape_slot_of(id: GlobalShapeId, key: &str) -> Option<usize> {
    let idx = id.checked_sub(GLOBAL_SHAPE_BASE)?;
    let reg = registry().read().expect(POISON);
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
