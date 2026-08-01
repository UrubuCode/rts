//! Kernel OBJ — dictionary vs shape, on the SAME property read.
//!
//! CLAUDE.md says property access is "shape-id compare + fixed-offset load (not
//! hash lookup)". Both representations exist in the tree, and which one a read
//! takes depends on how the receiver was built. This kernel prices all three
//! points on that spectrum with one identical workload:
//!
//! - **O0** `Entry::Map(IndexMap<String,i64>)` reached by `__rtsadp_obj_get`,
//!   including the `key_text` step that allocates an owned `String` PER READ.
//! - **O1** the same map and the same lock, key already interned — isolates what
//!   `key_text` costs from what the hash lookup costs.
//! - **O2** shape guard + fixed-offset load (kernel A's A4g), i.e. the thing the
//!   doc describes.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, types};

use super::{Compiled, call1, emit_unbox_double, loop_kernel};

const DICT_GET: (&str, usize) = ("probe_dict_get", 2);
const DICT_GET_BORROWED: (&str, usize) = ("probe_dict_get_borrowed", 2);

/// `mask` carries the key (a string-handle payload for O0, a `&&str` pointer for
/// O1) instead of an index mask — this kernel reads ONE property from one
/// dictionary, so there is nothing to index.
pub fn o0_dict_keyed() -> Compiled {
    loop_kernel("obj_o0_dict_keyed", &[DICT_GET], types::F64, |fb, im, v| {
        // `v.idx` is `i & key`, which is not what we want here; use the raw key
        // that the driver placed in the arena-base slot.
        let w = call1(fb, im["probe_dict_get"], &[v.payload, v.arena_base]);
        let f = emit_unbox_double(fb, w);
        fb.ins().fadd(v.s, f)
    })
}

pub fn o1_dict_interned_key() -> Compiled {
    loop_kernel(
        "obj_o1_dict_interned_key",
        &[DICT_GET_BORROWED],
        types::F64,
        |fb, im, v| {
            let w = call1(fb, im["probe_dict_get_borrowed"], &[v.payload, v.arena_base]);
            let f = emit_unbox_double(fb, w);
            fb.ins().fadd(v.s, f)
        },
    )
}

/// O2 — the same single property read as a shape guard + fixed-offset load.
/// `payload` is the object's arena offset; slot 0 is the shape id.
pub fn o2_shape_load(shape_id: i64) -> Compiled {
    loop_kernel("obj_o2_shape_load", &[], types::F64, move |fb, _im, v| {
        let trusted = MemFlags::trusted();
        let byte_off = fb.ins().imul_imm(v.payload, 8);
        let obj = fb.ins().iadd(v.arena_base, byte_off);
        let shape = fb.ins().load(types::I64, trusted, obj, 0);
        let want = fb.ins().iconst(types::I64, shape_id);
        let hit = fb.ins().icmp(IntCC::Equal, shape, want);

        let fast = fb.create_block();
        let miss = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(hit, fast, &[], miss, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let w = fb.ins().load(types::I64, trusted, obj, 8);
        let f = emit_unbox_double(fb, w);
        let s1 = fb.ins().fadd(v.s, f);
        fb.ins().jump(cont, &[s1.into()]);

        fb.switch_to_block(miss);
        fb.seal_block(miss);
        let nanv = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(v.s, nanv);
        fb.ins().jump(cont, &[s2.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    })
}
