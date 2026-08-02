//! The benchmark drivers, one module per primordial under test.

pub mod hermes;
pub mod methods;
pub mod numwidth;
pub mod objects;
pub mod ops;
pub mod repr;
pub mod stdlib;
pub mod stdlibshape;
pub mod strings;
pub mod structs;
pub mod arch;
pub mod backend;
pub mod closing;
pub mod compiletime;
pub mod complexity;
pub mod final_two;
pub mod irladder;
pub mod symarch;
pub mod visible;
pub mod types;
pub mod writes;

/// Objects/elements live in a power-of-two ring so `i & MASK` indexes them;
/// nothing is loop-invariant, so no variant can win by having a load hoisted.
pub const N_OBJS: usize = 1024;
pub const MASK: i64 = (N_OBJS as i64) - 1;

/// A narrower ring for the integer kernel: it keeps the running sum inside i32
/// range so the inline int32 arm stays on its fast path for the whole loop
/// instead of silently degrading to the generic call halfway through.
pub const MASK_PRIM: i64 = 255;

pub const ITERS_A: i64 = 3_000_000; // same count as bench/objbench.ts
pub const ITERS_B: i64 = 1_000_000; // 1 allocation per iteration
pub const ITERS_C: i64 = 20_000_000; // pure arithmetic, no heap
pub const ITERS_ARR: i64 = 3_000_000;
pub const ITERS_OBJ: i64 = 3_000_000;
pub const ITERS_PRIM: i64 = 20_000_000;
pub const ITERS_PRIM_INT: i64 = 2_000_000;
pub const ITERS_OPS: i64 = 10_000_000;
/// Kernel W allocates ONE object per iteration, so its count is bounded by slab
/// capacity, not by how long a loop we want: W4's single-region row puts every
/// object in `moving_slab`'s region 0, whose `CAP_PER_SHARD` is 65 536. That is a
/// probe capacity limit, not a statement about the design — but a row that
/// asserted its way out mid-run would measure nothing at all.
pub const ITERS_W: i64 = 50_000;

pub const SHAPE_ID: i64 = 7;
