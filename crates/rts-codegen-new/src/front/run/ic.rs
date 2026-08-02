//! Property inline caches — the FAST-PATH half (the emitter).
//!
//! A property read whose receiver shape is statically proven lowers to a
//! constant-slot load and is already fast. A read whose receiver is NOT proven —
//! an `any`-typed param, a `.map(q => q.x)` callback param, a call return — calls
//! `__rtsadp_obj_get`, which takes the PROCESS-GLOBAL shape-registry mutex and
//! scans the shape's key list on every single access.
//!
//! Measured (release, 1M iterations, vs bun 1.3.14): the untyped read costs
//! 436 ms where the identical read through a nominally-typed receiver costs
//! 13 ms — 33× apart on the same work, and the untyped form is ordinary
//! TypeScript, not an edge case.
//!
//! This module emits, per CALL SITE, a check against a small writable data cell
//! (see [`crate::adapters_ic_docs`] — the layout and the soundness argument live
//! with the miss handler in `rts_runtime::adapters::value::ic`):
//!
//! ```text
//!   if is_object(recv) && slot0(recv) == cell.shape && len(recv) == cell.len {
//!       value = slot(recv, cell.slot)            // no mutex, no key scan
//!   } else {
//!       value = __rtsadp_ic_miss(recv, key, &cell)   // resolve + refill
//!   }
//! ```
//!
//! The cell is DATA, never patched code, so the identical emission serves the
//! JIT and an AOT object file. It is zero-initialised and a zeroed cell can never
//! hit: a real `shape` word is a boxed tagged value, so it is never `0`.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{DataDescription, Linkage, Module};

use crate::front::error::{FrontResult, Unsupported};
use crate::value::{self, emit_marshal};

/// Cell layout, in bytes. Mirrors `rts_runtime::adapters::value::ic::IC_CELL_BYTES`.
const IC_CELL_BYTES: usize = 24;
const OFF_SHAPE: i32 = 0;
const OFF_LEN: i32 = 8;
const OFF_SLOT: i32 = 16;

thread_local! {
    /// IC-site index PER OWNING FUNCTION, so a cell's symbol name is
    /// `__rtsic_<fn>_<site>` — derived from where the site IS, not from how many
    /// sites happened to be lowered before it.
    ///
    /// This replaced a single process-wide counter, and the reason is measured.
    /// A cell name is baked into the owning function's IR, so a global counter
    /// made a function's IR depend on the whole rest of the compile: prelude
    /// PRUNING keeps a different subset per program, which shifted every
    /// downstream cell number, which gave the SAME prelude source a different
    /// Cranelift stencil in every process. That is what made the incremental
    /// cache (`super::clifcache`) unable to hit across processes — see its module
    /// doc for the suite numbers. Per-function numbering removes the coupling:
    /// the third IC site in `Array.prototype.map` is `__rtsic_..._2` no matter
    /// what else the program contains.
    ///
    /// Uniqueness still holds — function names are unique within a module and the
    /// index is unique within a function — which is all `declare_data` requires.
    static SITE_CTR: std::cell::RefCell<std::collections::HashMap<String, u32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Next IC-site index for `owner`, counting from 0 per function.
fn next_site(owner: &str) -> u32 {
    SITE_CTR.with(|m| {
        let mut m = m.borrow_mut();
        let slot = m.entry(owner.to_string()).or_insert(0);
        let id = *slot;
        *slot += 1;
        id
    })
}

/// Clear the per-function site counters. Called once per program compile: the
/// names must restart at 0 for each module, or a second compile in the same
/// process would number its sites from where the first one stopped — reintroducing
/// exactly the cross-compile coupling this design removes.
pub(super) fn reset_sites() {
    SITE_CTR.with(|m| m.borrow_mut().clear());
}

/// Declare one zero-initialised, WRITABLE cell and return a pointer to it.
///
/// `writable = true` is the only difference from `aot_str::emit_str_data`'s
/// read-only blob; the declaration, the `declare_data_in_func` reference and the
/// baker capture are the same mechanism, so nothing new is required of the AOT
/// or replay paths.
fn declare_cell(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    owner: &str,
) -> FrontResult<Value> {
    let id = next_site(owner);
    let name = format!("__rtsic_{owner}_{id}");
    let data_id = crate::timing::declare(|| {
        module
            .declare_data(&name, Linkage::Local, true, false)
            .map_err(|e| Unsupported::new(format!("declare ic cell `{name}`: {e}")))
    })?;
    let mut desc = DataDescription::new();
    desc.define_zeroinit(IC_CELL_BYTES);
    crate::timing::declare(|| {
        module
            .define_data(data_id, &desc)
            .map_err(|e| Unsupported::new(format!("define ic cell `{name}`: {e}")))
    })?;
    // Baker capture: a replayed program re-declares the cell COLD (all zeros) and
    // re-warms on first use. A warm cell must never be baked — its shape ids are
    // only meaningful against the registry the code was compiled with. It must be
    // re-declared WRITABLE too: `__rtsadp_ic_miss` fills the cell in place, so a
    // read-only replay of it faults on the first miss.
    super::aot_str::capture_data_blob(&name, vec![0u8; IC_CELL_BYTES], true);
    let gv = module.declare_data_in_func(data_id, builder.func);
    Ok(builder.ins().global_value(types::I64, gv))
}

/// `is_object(word)` as pure IR: boxed AND tag == OBJECT.
fn emit_is_object(builder: &mut FunctionBuilder, v: Value) -> Value {
    let boxed = value::emit_is_boxed(builder, v);
    let shifted = builder.ins().ushr_imm(v, value::TAG_SHIFT as i64);
    let tag = builder.ins().band_imm(shifted, value::TAG_MASK as i64);
    let is_obj_tag = builder
        .ins()
        .icmp_imm(IntCC::Equal, tag, value::TAG_OBJECT as i64);
    builder.ins().band(boxed, is_obj_tag)
}

/// Emit a cached dynamic property read of `key_word` on `recv_word`, returning
/// the value word.
///
/// Semantically identical to calling `__rtsadp_obj_get(recv_word, key_word)` —
/// the miss path IS that call, and the hit path is only taken when the guard
/// proves the receiver is the same shaped object the site last resolved against.
pub(super) fn emit_cached_get(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    recv_word: Value,
    key_word: Value,
    owner: &str,
) -> FrontResult<Value> {
    let cell = declare_cell(module, builder, owner)?;

    let hit_blk = builder.create_block();
    let miss_blk = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    // Guard. `MemFlags::trusted` is sound here: the cell is module-owned data of
    // known size and alignment, live for the whole program.
    let flags = MemFlags::trusted();
    let cached_shape = builder.ins().load(types::I64, flags, cell, OFF_SHAPE);
    let is_obj = emit_is_object(builder, recv_word);
    // A cold cell holds 0, which no real shape word can equal, so the `is_object`
    // check is what keeps a NON-object receiver from ever reading slot 0 of
    // something that is not a vec — `vec_get` would answer 0 for it and a cold
    // cell would then match. Order: object first, content second.
    let zero = builder.ins().iconst(types::I64, 0);
    let idx0 = builder.ins().iconst(types::I64, 0);
    let slot0 = emit_marshal::emit_vec_get(module, builder, recv_word, idx0);
    let len = emit_marshal::emit_vec_len(module, builder, recv_word);
    let cached_len = builder.ins().load(types::I64, flags, cell, OFF_LEN);

    let warm = builder.ins().icmp(IntCC::NotEqual, cached_shape, zero);
    let same_shape = builder.ins().icmp(IntCC::Equal, slot0, cached_shape);
    let same_len = builder.ins().icmp(IntCC::Equal, len, cached_len);
    let c1 = builder.ins().band(is_obj, warm);
    let c2 = builder.ins().band(c1, same_shape);
    let ok = builder.ins().band(c2, same_len);
    // COLD: the miss is taken on the FIRST execution of the site and then only
    // when the receiver changes shape — the whole premise of an inline cache is
    // that a site is monomorphic, and the measured 436 ms -> 13 ms gap is the hit
    // path. The miss is also the non-fallthrough edge of the `brif` (`hit_blk` is
    // filled first, so Cranelift lays it out immediately after the branch), which
    // is the layout the hint is worth anything in. `merge` is NOT marked: both
    // edges join there.
    if super::clifflags::cold_blocks() {
        builder.set_cold_block(miss_blk);
    }
    builder.ins().brif(ok, hit_blk, &[], miss_blk, &[]);
    builder.seal_block(hit_blk);
    builder.seal_block(miss_blk);

    // HIT: constant-slot load, no mutex, no scan, no allocation.
    builder.switch_to_block(hit_blk);
    let slot = builder.ins().load(types::I64, flags, cell, OFF_SLOT);
    let hit_val = emit_marshal::emit_vec_get(module, builder, recv_word, slot);
    builder.ins().jump(merge, &[hit_val.into()]);

    // MISS: the generic read is the authority; it also refills the cell when the
    // receiver turns out to be a shaped keyed object carrying an own slot.
    builder.switch_to_block(miss_blk);
    let miss_val = emit_marshal::emit_call(
        module,
        builder,
        "__rtsadp_ic_miss",
        &[recv_word, key_word, cell],
    )
    .expect("__rtsadp_ic_miss returns a value");
    builder.ins().jump(merge, &[miss_val.into()]);

    builder.switch_to_block(merge);
    builder.seal_block(merge);
    Ok(builder.block_params(merge)[0])
}
