//! AOT seed blob (compiler ↔ runtime, in-process for JIT, cross-process for AOT).
//!
//! The global shape registry is populated at COMPILE time in the compiler process,
//! and shape ids are baked as IMMEDIATES into the emitted code (slot-0 of every
//! object, the compare arms of dynamic dispatch). The JIT shares the registry with
//! the run (same process), so `global_shape_keys(baked_id)` resolves. An AOT binary
//! is a SEPARATE process whose registry starts EMPTY — every DYNAMIC shape read
//! (`__rtsadp_obj_get` on a Tagged/`any`/catch-bound receiver, `console.log(obj)`,
//! dynamic `Object.keys`) then misses and returns `undefined`. This is the same
//! class of bug as the string-pool handles (fixed by making key immediates
//! AOT-safe): a compile-time value baked into code that means nothing in the AOT
//! process. Here the fix transfers the id→keys registry itself: the AOT `main` shim
//! embeds this blob and calls `__RTS_FN_RT_SEED_SHAPES` before `__rts_startup`.
//!
//! Format (little-endian, length-prefixed; a PRIVATE contract — both sides are this
//! module): u32 num_shapes, then per shape { u32 num_keys, per key { u32 len, bytes }};
//! u32 num_errs, then per err { u32 name_len, name_bytes, u32 shape_id, u32
//! num_fields, per field { u32 len, bytes } }.

use super::GlobalShapeId;
use super::classes::{
    export_class_shapes, export_error_classes, seed_class_shapes, seed_error_classes,
};
use super::registry::{export_global_shapes, reset_global_shapes, seed_global_shapes};

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
    // Sequential, never nested: each of these takes and releases its own lock,
    // so the seed path is outside the registry→classes nesting entirely.
    reset_global_shapes();
    seed_global_shapes(shapes);
    seed_error_classes(errs);
    seed_class_shapes(classes);
}
