# Cranelift — Complete Library Reference

**Version:** 0.131.0  
**Crates:** `cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`, `cranelift-object`, `cranelift-native`

This document is a standalone reference for using Cranelift as a code generation
backend. It covers every major capability: types, instructions, memory, control
flow, calling conventions, atomics, SIMD, and GC integration. Not specific to any
project — general Cranelift usage.

---

## Table of Contents

1. [Crate Overview](#1-crate-overview)
2. [Type System](#2-type-system)
3. [Setup — JIT and AOT Modules](#3-setup--jit-and-aot-modules)
4. [Function Builder](#4-function-builder)
5. [Constants](#5-constants)
6. [Integer Arithmetic](#6-integer-arithmetic)
7. [Bitwise and Shift Operations](#7-bitwise-and-shift-operations)
8. [Floating-Point Arithmetic](#8-floating-point-arithmetic)
9. [Type Conversions](#9-type-conversions)
10. [Comparisons](#10-comparisons)
11. [Control Flow](#11-control-flow)
12. [Memory Operations](#12-memory-operations)
13. [Stack Slots](#13-stack-slots)
14. [Global Values and Static Data](#14-global-values-and-static-data)
15. [Function Signatures and Calling Conventions](#15-function-signatures-and-calling-conventions)
16. [Function Calls](#16-function-calls)
17. [Select (Branchless Conditional)](#17-select-branchless-conditional)
18. [Specialized Arithmetic](#18-specialized-arithmetic)
19. [Atomic Operations](#19-atomic-operations)
20. [SIMD / Vector Operations](#20-simd--vector-operations)
21. [GC Stack Maps](#21-gc-stack-maps)
22. [Optimization Settings](#22-optimization-settings)
23. [Common Patterns](#23-common-patterns)
24. [Pitfalls and Rules](#24-pitfalls-and-rules)
25. [Exception Handling — `try_call`](#25-exception-handling--try_call-and-exception-tables)
26. [Libcalls](#26-libcalls--runtime-routines-the-backend-may-emit)
27. [Cold Blocks and Layout Hints](#27-cold-blocks-and-layout-hints)

---

## 1. Crate Overview

```toml
cranelift-codegen  = "0.131"   # Core IR, types, instructions, settings
cranelift-frontend = "0.131"   # FunctionBuilder — high-level IR construction
cranelift-jit      = "0.131"   # JIT: compile to executable memory
cranelift-module   = "0.131"   # Module abstraction, DataId, FuncId, Linkage
cranelift-object   = "0.131"   # AOT: emit ELF / Mach-O / COFF object files
cranelift-native   = "0.131"   # Auto-detect host ISA (x86-64, aarch64, etc.)
```

### Key import paths

```rust
use cranelift_codegen::ir::{
    types,               // I8, I16, I32, I64, I128, F32, F64, F128, ...
    InstBuilder,         // trait that provides all ins().* methods
    MemFlags,
    condcodes::{IntCC, FloatCC},
    AbiParam, Signature, Type, Value, Block, StackSlot,
    GlobalValue,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable, Switch};
use cranelift_module::{Module, Linkage, DataDescription, FuncId, DataId};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_object::{ObjectBuilder, ObjectModule};
use cranelift_native;
```

---

## 2. Type System

All types live in `cranelift_codegen::ir::types`.

### Integer Types

Cranelift does not distinguish signed from unsigned at the type level.
Signedness is encoded in the **instruction** (e.g., `sdiv` vs `udiv`,
`sextend` vs `uextend`, `IntCC::SignedLessThan` vs `IntCC::UnsignedLessThan`).

| Type constant | Bits | Notes |
|---|---|---|
| `types::I8`  | 8  | byte; also used as boolean (0/1) by some instructions |
| `types::I16` | 16 | halfword |
| `types::I32` | 32 | word; pointer type on 32-bit targets |
| `types::I64` | 64 | doubleword; pointer type on 64-bit targets |
| `types::I128`| 128| two registers on most backends |

### Float Types

| Type constant | Bits | Standard |
|---|---|---|
| `types::F32` | 32 | IEEE 754 single precision |
| `types::F64` | 64 | IEEE 754 double precision |
| `types::F128`| 128| IEEE 754 quadruple precision (limited backend support) |

### Vector / SIMD Types

Vector types encode the **element type × lane count** as a flat token:

| Element | 2 lanes | 4 lanes | 8 lanes | 16 lanes |
|---|---|---|---|---|
| I8  | — | `I8X4` | `I8X8` | `I8X16` |
| I16 | — | `I16X4`| `I16X8`| — |
| I32 | `I32X2`| `I32X4`| — | — |
| I64 | `I64X2`| — | — | — |
| F32 | — | `F32X4`| — | — |
| F64 | `F64X2`| — | — | — |

Practical vector type for SIMD-128 (SSE2 / NEON width):

```rust
let v128_i8  = types::I8X16;
let v128_i32 = types::I32X4;
let v128_f32 = types::F32X4;
let v128_f64 = types::F64X2;
```

### Get Pointer Width

```rust
let ptr_ty: Type = module.isa().pointer_type(); // I32 on 32-bit, I64 on 64-bit
```

---

## 3. Setup — JIT and AOT Modules

### Common: build ISA with settings

```rust
let mut flag_builder = settings::builder();
flag_builder.set("opt_level", "speed").unwrap();       // "none" | "speed" | "speed_and_size"
flag_builder.set("is_pic", "false").unwrap();          // position-independent code
flag_builder.set("preserve_frame_pointers", "true").unwrap(); // required for tail calls on x86-64
flag_builder.set("use_egraphs", "true").unwrap();      // e-graph optimizer (recommended)
flag_builder.set("enable_alias_analysis", "true").unwrap();
let flags = settings::Flags::new(flag_builder);

let isa = cranelift_native::builder()
    .expect("unsupported host arch")
    .finish(flags)
    .unwrap();
```

### JIT Module

```rust
let mut jit_builder = JITBuilder::with_isa(
    isa,
    cranelift_module::default_libcall_names(), // soft-float fallbacks etc.
);

// Register every extern "C" symbol the JIT code will call
jit_builder.symbol("__MY_RUNTIME_FN", my_fn as *const u8);

let mut module = JITModule::new(jit_builder);
```

### AOT (Object File) Module

```rust
let object_builder = ObjectBuilder::new(
    isa,
    b"my_module",
    cranelift_module::default_libcall_names(),
).unwrap();

let mut module = ObjectModule::new(object_builder);
```

---

## 4. Function Builder

### Declare and start building a function

```rust
// 1. Declare function in module
let sig = make_signature(&module);   // see §15
let func_id = module.declare_function("my_fn", Linkage::Local, &sig).unwrap();

// 2. Create Context (holds the IR function)
let mut ctx = module.make_context();
ctx.func.signature = sig.clone();

// 3. Create FunctionBuilderContext (reusable across functions)
let mut fb_ctx = FunctionBuilderContext::new();

// 4. Build function body
{
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    // ... emit instructions ...

    builder.finalize();
}

// 5. Define function in module
module.define_function(func_id, &mut ctx).unwrap();
module.clear_context(&mut ctx);
```

### Variables (persistent across blocks — wraps phi nodes)

```rust
let var = Variable::new(0);                    // unique index
builder.declare_var(var, types::I64);          // declare before use

builder.def_var(var, initial_value);           // write
let v = builder.use_var(var);                  // read (auto-inserts phi if needed)
```

### Blocks

```rust
let block = builder.create_block();

// Add a block parameter (SSA phi input):
let param: Value = builder.append_block_param(block, types::I64);

builder.switch_to_block(block);
builder.seal_block(block);   // call after all predecessors are wired
```

**Rule:** every block must be sealed exactly once, after all jumps into it
are emitted. Sealing triggers phi reduction.

---

## 5. Constants

```rust
// Integer constants
let c0  = builder.ins().iconst(types::I8,  42_i64);
let c1  = builder.ins().iconst(types::I16, 1000_i64);
let c2  = builder.ins().iconst(types::I32, -1_i64);    // all-ones bit pattern
let c3  = builder.ins().iconst(types::I64, 0x_DEAD_BEEF_i64);

// Float constants
let f32v = builder.ins().f32const(3.14_f32);
let f64v = builder.ins().f64const(std::f64::consts::PI);

// Null pointer
let null = builder.ins().null(types::I64);
```

---

## 6. Integer Arithmetic

All arithmetic instructions produce a value of the **same type** as the inputs.
Inputs must match types.

### Binary — full register operands

```rust
// Add / Subtract
let s = builder.ins().iadd(a, b);      // a + b
let d = builder.ins().isub(a, b);      // a - b

// Multiply
let p = builder.ins().imul(a, b);      // a * b (low N bits)

// Signed division and remainder
let q  = builder.ins().sdiv(a, b);     // a / b (signed, traps on div-by-zero)
let r  = builder.ins().srem(a, b);     // a % b (signed; sign follows dividend)

// Unsigned division and remainder
let uq = builder.ins().udiv(a, b);     // a / b (unsigned)
let ur = builder.ins().urem(a, b);     // a % b (unsigned)

// Negate
let neg = builder.ins().ineg(a);       // -a (two's complement)
```

### Binary — immediate forms (avoid an `iconst`)

Immediate `imm: i64` is sign-extended to match the operand type.

```rust
let v = builder.ins().iadd_imm(a, 8);      // a + 8
let v = builder.ins().imul_imm(a, 4);      // a * 4
let v = builder.ins().band_imm(a, 0xFF);   // a & 0xFF
let v = builder.ins().bor_imm(a, 0x01);    // a | 1
let v = builder.ins().bxor_imm(a, -1_i64); // a ^ ~0 (bitwise NOT shortcut)
let v = builder.ins().ishl_imm(a, 3);      // a << 3
let v = builder.ins().ushr_imm(a, 3);      // a >> 3 (unsigned)
let v = builder.ins().sshr_imm(a, 3);      // a >> 3 (signed/arithmetic)
```

### Overflow-aware arithmetic (returns `(value, bool)`)

```rust
// Returns (result: Value, overflow_flag: Value)
let (res, of) = builder.ins().uadd_overflow(a, b);   // unsigned add
let (res, of) = builder.ins().sadd_overflow(a, b);   // signed add
let (res, of) = builder.ins().usub_overflow(a, b);   // unsigned sub
let (res, of) = builder.ins().ssub_overflow(a, b);   // signed sub
let (res, of) = builder.ins().umul_overflow(a, b);   // unsigned mul
```

---

## 7. Bitwise and Shift Operations

```rust
// Bitwise binary
let and = builder.ins().band(a, b);    // a & b
let or  = builder.ins().bor(a, b);    // a | b
let xor = builder.ins().bxor(a, b);   // a ^ b
let not = builder.ins().bnot(a);      // ~a

// Shifts (rhs = shift amount, same type as lhs)
let shl  = builder.ins().ishl(a, n);  // a << n (logical)
let lshr = builder.ins().ushr(a, n);  // a >> n (logical, zero-fill)
let ashr = builder.ins().sshr(a, n);  // a >> n (arithmetic, sign-fill)

// Rotations
let rl = builder.ins().rotl(a, n);    // rotate left
let rr = builder.ins().rotr(a, n);    // rotate right

// Bit counting
let lz  = builder.ins().clz(a);      // count leading zeros
let tz  = builder.ins().ctz(a);      // count trailing zeros
let pc  = builder.ins().popcnt(a);   // population count (number of 1-bits)
let cls = builder.ins().cls(a);      // count leading sign bits

// Byte swap (endian reversal)
let bs = builder.ins().bswap(a);     // reverse byte order (I16, I32, I64, I128)
```

### Common strength-reduction patterns

```rust
// Multiply by power of 2 → shift
if imm.is_power_of_two() {
    builder.ins().ishl_imm(a, imm.trailing_zeros() as i64)
} else {
    builder.ins().imul_imm(a, imm)
}

// Unsigned modulo by power of 2 → mask
if imm.is_power_of_two() {
    builder.ins().band_imm(a, imm - 1)
} else {
    builder.ins().urem(a, builder.ins().iconst(ty, imm))
}

// Signed divide by power of 2 → arithmetic shift
if imm.is_power_of_two() {
    builder.ins().sshr_imm(a, imm.trailing_zeros() as i64)
}
```

---

## 8. Floating-Point Arithmetic

```rust
// Basic arithmetic
let add = builder.ins().fadd(a, b);
let sub = builder.ins().fsub(a, b);
let mul = builder.ins().fmul(a, b);
let div = builder.ins().fdiv(a, b);   // IEEE-754; never traps (returns NaN/Inf)

// Fused multiply-add: (a * b) + c  — single rounding, no intermediate loss
let fma = builder.ins().fma(a, b, c);

// Unary
let abs = builder.ins().fabs(a);      // |a|
let neg = builder.ins().fneg(a);      // -a (flip sign bit)
let sqr = builder.ins().sqrt(a);      // √a (IEEE-754)

// Copy sign: result = magnitude of |a| with sign of b
let cs = builder.ins().fcopysign(a, b);

// Min/Max (IEEE-754 2019 semantics — NaN propagates)
let mn = builder.ins().fmin(a, b);
let mx = builder.ins().fmax(a, b);

// Rounding
let up   = builder.ins().ceil(a);      // round toward +∞
let dn   = builder.ins().floor(a);     // round toward -∞
let zero = builder.ins().trunc(a);     // round toward 0
let near = builder.ins().nearest(a);   // round to nearest, ties to even
```

---

## 9. Type Conversions

### Integer ↔ Integer

```rust
// Truncate (reduce width): high bits discarded
let small = builder.ins().ireduce(types::I32, i64_val);   // I64 → I32
let byte  = builder.ins().ireduce(types::I8,  i32_val);   // I32 → I8

// Zero-extend (unsigned widen)
let wide  = builder.ins().uextend(types::I64, i32_val);   // I32 → I64 (zero-fill)
let wide  = builder.ins().uextend(types::I32, i8_val);    // I8  → I32

// Sign-extend (signed widen)
let wide  = builder.ins().sextend(types::I64, i32_val);   // I32 → I64 (sign-fill)
let wide  = builder.ins().sextend(types::I64, i8_val);    // I8  → I64
```

### Integer ↔ Float

```rust
// Integer → Float (no precision loss for small integers)
let f64v = builder.ins().fcvt_from_sint(types::F64, i64_val);  // i64 → f64
let f32v = builder.ins().fcvt_from_sint(types::F32, i32_val);  // i32 → f32
let f64v = builder.ins().fcvt_from_uint(types::F64, u64_val);  // u64 → f64

// Float → Integer (saturating — clamps instead of trapping on overflow)
let i64v = builder.ins().fcvt_to_sint_sat(types::I64, f64_val); // f64 → i64
let u64v = builder.ins().fcvt_to_uint_sat(types::I64, f64_val); // f64 → u64

// Float → Integer (trapping — traps on NaN or overflow)
let i64v = builder.ins().fcvt_to_sint(types::I64, f64_val);
let u64v = builder.ins().fcvt_to_uint(types::I64, f64_val);
```

### Float ↔ Float

```rust
// Promote: f32 → f64 (lossless)
let f64v = builder.ins().fpromote(types::F64, f32_val);

// Demote: f64 → f32 (may lose precision)
let f32v = builder.ins().fdemote(types::F32, f64_val);
```

### Bitcast (reinterpret bits, no conversion)

```rust
// i64 bits viewed as f64 (transmute equivalent)
let f64v = builder.ins().bitcast(types::F64, MemFlags::new(), i64_val);
let i64v = builder.ins().bitcast(types::I64, MemFlags::new(), f64_val);

// i32 bits as f32
let f32v = builder.ins().bitcast(types::F32, MemFlags::new(), i32_val);
```

---

## 10. Comparisons

### Integer comparison — `icmp`

```rust
// Returns I8 (0 or 1). Use brif, select, or extend as needed.
let eq  = builder.ins().icmp(IntCC::Equal,                    a, b);
let ne  = builder.ins().icmp(IntCC::NotEqual,                 a, b);

// Signed
let slt = builder.ins().icmp(IntCC::SignedLessThan,           a, b);
let sle = builder.ins().icmp(IntCC::SignedLessThanOrEqual,    a, b);
let sgt = builder.ins().icmp(IntCC::SignedGreaterThan,        a, b);
let sge = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b);

// Unsigned
let ult = builder.ins().icmp(IntCC::UnsignedLessThan,           a, b);
let ule = builder.ins().icmp(IntCC::UnsignedLessThanOrEqual,    a, b);
let ugt = builder.ins().icmp(IntCC::UnsignedGreaterThan,        a, b);
let uge = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, a, b);

// With immediate
let z   = builder.ins().icmp_imm(IntCC::Equal, a, 0);
```

### Float comparison — `fcmp`

```rust
// Ordered: returns false if either operand is NaN
let eq  = builder.ins().fcmp(FloatCC::Equal,                      a, b);
let ne  = builder.ins().fcmp(FloatCC::NotEqual,                   a, b);
let olt = builder.ins().fcmp(FloatCC::OrderedLessThan,            a, b);
let ole = builder.ins().fcmp(FloatCC::OrderedLessThanOrEqual,     a, b);
let ogt = builder.ins().fcmp(FloatCC::OrderedGreaterThan,         a, b);
let oge = builder.ins().fcmp(FloatCC::OrderedGreaterThanOrEqual,  a, b);
let one = builder.ins().fcmp(FloatCC::OrderedNotEqual,            a, b);

// Unordered: returns true if either operand is NaN
let ult = builder.ins().fcmp(FloatCC::UnorderedLessThan,           a, b);
let ule = builder.ins().fcmp(FloatCC::UnorderedLessThanOrEqual,    a, b);
let ugt = builder.ins().fcmp(FloatCC::UnorderedGreaterThan,        a, b);
let uge = builder.ins().fcmp(FloatCC::UnorderedGreaterThanOrEqual, a, b);
let une = builder.ins().fcmp(FloatCC::UnorderedNotEqual,           a, b);

// NaN checks
let is_nan     = builder.ins().fcmp(FloatCC::Unordered, a, a); // always true if NaN
let is_not_nan = builder.ins().fcmp(FloatCC::Ordered,   a, a); // always true if not NaN
```

---

## 11. Control Flow

### Unconditional jump

```rust
builder.ins().jump(target_block, &[arg1, arg2]);  // args become block params
```

### Conditional branch

```rust
// cond is any I8/I64 value; non-zero = taken
builder.ins().brif(
    cond,
    then_block, &[then_arg1],  // args for then_block params
    else_block, &[else_arg1],  // args for else_block params
);
```

### Multi-way branch (switch / jump table)

Two approaches:

**A) `Switch` builder** (high-level — backend chooses `br_table` vs binary search):

```rust
let mut sw = Switch::new();
sw.set_entry(0, case0_block);
sw.set_entry(1, case1_block);
sw.set_entry(42, case42_block);
sw.emit(&mut builder, index_value, default_block);
```

**B) `br_table`** (low-level — explicit jump table):

```rust
let jt_data = JumpTableData::new(default_block, &[case0, case1, case2]);
let jt = builder.create_jump_table(jt_data);
builder.ins().br_table(index, default_block, jt);
```

### Return

```rust
builder.ins().return_(&[]);               // void
builder.ins().return_(&[value]);          // one return value
builder.ins().return_(&[v1, v2]);         // multiple return values
```

### Traps

```rust
use cranelift_codegen::ir::TrapCode;

builder.ins().trap(TrapCode::IntegerOverflow);    // unconditional trap
builder.ins().trapz(cond, TrapCode::IntegerDivisionByZero);   // trap if cond == 0
builder.ins().trapnz(cond, TrapCode::HeapOutOfBounds);        // trap if cond != 0
```

Common `TrapCode` variants:
- `StackOverflow`
- `HeapOutOfBounds`
- `TableOutOfBounds`
- `IntegerOverflow`
- `IntegerDivisionByZero`
- `BadSignature`
- `UnreachableCodeReached`
- `User(u16)` — custom trap code

---

## 12. Memory Operations

### MemFlags

```rust
MemFlags::new()        // default — may trap, not necessarily aligned
MemFlags::trusted()    // pointer is always valid and aligned (enables hoisting)
MemFlags::notrap()     // non-trapping load (safe to speculate)

// Combinable:
let flags = MemFlags::new().with_notrap().with_aligned().with_readonly();
```

Use `trusted()` for stack slots and known globals. Use `notrap()` only when
you've validated the pointer in the language runtime.

### Alias regions — disambiguate memory for the optimizer

A `MemFlags` can be tagged with an **alias region** so the e-graph's alias
analysis knows two accesses can't overlap and may reorder/eliminate them. Regions
are mutually disjoint by assumption: a `Heap` load never aliases a `Table` or
`Vmctx` store.

```rust
use cranelift_codegen::ir::AliasRegion;

let heap  = MemFlags::new().with_alias_region(Some(AliasRegion::Heap));   // GC heap / linear memory
let table = MemFlags::new().with_alias_region(Some(AliasRegion::Table));  // handle/function tables
let vmctx = MemFlags::trusted().with_alias_region(Some(AliasRegion::Vmctx)); // runtime context struct
```

Only ONE region per flag set (setting a second is an error). Tag a load/store with
a region only when the memory genuinely cannot alias the others — a wrong tag lets
the optimizer drop a real dependency and miscompile. Untagged accesses (`None`,
the default) conservatively alias everything.

### Full-width load / store

```rust
// Load: read value_type from ptr + byte_offset
let v = builder.ins().load(types::I64, MemFlags::trusted(), ptr, 0);
let v = builder.ins().load(types::F64, MemFlags::trusted(), ptr, 8);

// Store: write value to ptr + byte_offset
builder.ins().store(MemFlags::trusted(), value, ptr, 0);
```

### Narrow loads (sub-word)

All zero-extend or sign-extend to the natural register width (I64 on 64-bit).

```rust
// Zero-extending
let v = builder.ins().uload8(MemFlags::trusted(), ptr, 0);   // u8  → i64
let v = builder.ins().uload16(MemFlags::trusted(), ptr, 0);  // u16 → i64
let v = builder.ins().uload32(MemFlags::trusted(), ptr, 0);  // u32 → i64

// Sign-extending
let v = builder.ins().sload8(MemFlags::trusted(), ptr, 0);   // i8  → i64
let v = builder.ins().sload16(MemFlags::trusted(), ptr, 0);  // i16 → i64
let v = builder.ins().sload32(MemFlags::trusted(), ptr, 0);  // i32 → i64
```

### Narrow stores (sub-word)

Store only the low N bits of a full-width value.

```rust
builder.ins().istore8(MemFlags::trusted(), val, ptr, 0);    // store byte
builder.ins().istore16(MemFlags::trusted(), val, ptr, 0);   // store halfword
builder.ins().istore32(MemFlags::trusted(), val, ptr, 0);   // store word
```

### Pointer arithmetic

```rust
// Advance pointer by runtime offset
let new_ptr = builder.ins().iadd(ptr, offset);

// Advance pointer by compile-time offset (no extra iconst)
let new_ptr = builder.ins().iadd_imm(ptr, 16_i64);
```

---

## 13. Stack Slots

Stack slots are compile-time-sized regions on the current function's stack frame.

```rust
use cranelift_codegen::ir::{StackSlotData, StackSlotKind};

// Allocate: size in bytes, align as log2 (e.g. 3 = 8-byte alignment)
let slot: StackSlot = builder.create_sized_stack_slot(StackSlotData::new(
    StackSlotKind::ExplicitSlot,
    16,    // bytes
    3,     // align_log2 = 8-byte aligned
));

// Get address of slot (can add offset for field access)
let ptr_ty = module.isa().pointer_type();
let addr = builder.ins().stack_addr(ptr_ty, slot, 0);

// Read/write via pointer
let val = builder.ins().load(types::I64, MemFlags::trusted(), addr, 0);
builder.ins().store(MemFlags::trusted(), val, addr, 8);  // field at +8
```

Stack slots are faster than heap allocations — they live in the register
allocator's world and may be optimized away entirely.

---

## 14. Global Values and Static Data

### Declare and define mutable/immutable static data

```rust
// Declare symbol
let data_id: DataId = module.declare_data(
    "__MY_STATIC_VAR",
    Linkage::Local,    // Local | Export | Import
    true,              // writable (mutable)
    false,             // tls (thread-local storage)
).unwrap();

// Define content
let mut desc = DataDescription::new();
desc.define(vec![0u8; 8].into_boxed_slice());  // 8 bytes of zeros
module.define_data(data_id, &desc).unwrap();
```

### Access global from function IR

```rust
// Import DataId into function
let gv: GlobalValue = module.declare_data_in_func(data_id, builder.func);

// Get runtime address
let ptr_ty = module.isa().pointer_type();
let addr = builder.ins().global_value(ptr_ty, gv);

// Load / store as normal
let val = builder.ins().load(types::I64, MemFlags::trusted(), addr, 0);
builder.ins().store(MemFlags::trusted(), new_val, addr, 0);
```

### Declare external symbol (import)

```rust
let data_id = module.declare_data(
    "__EXTERNAL_SYM",
    Linkage::Import,
    false,
    false,
).unwrap();
// No define_data() call — linker resolves it
```

### Thread-local storage (TLS)

```rust
let tls_id = module.declare_data("__MY_TLS_VAR", Linkage::Local, true, true).unwrap();
// Use as normal; backend emits platform TLS access sequence
```

---

## 15. Function Signatures and Calling Conventions

### Build a signature

```rust
use cranelift_codegen::ir::{Signature, AbiParam};
use cranelift_codegen::isa::CallConv;

let mut sig = Signature::new(CallConv::Tail);
sig.params.push(AbiParam::new(types::I64));   // first param
sig.params.push(AbiParam::new(types::F64));   // second param
sig.returns.push(AbiParam::new(types::I64));  // single return
```

### Calling conventions

| `CallConv` variant | Use case |
|---|---|
| `CallConv::Tail` | User functions — enables `return_call` tail call optimization |
| `CallConv::SystemV` | Interop with C on Linux / macOS (AMD64 ABI) |
| `CallConv::WindowsFastcall` | Interop with C on Windows (Win64 ABI) |
| `CallConv::Cold` | Rarely-called paths (error handlers, slow paths) |
| `CallConv::Native` | Auto-select host platform convention |

**Rule:** use `CallConv::Tail` for all user-defined functions. Use `SystemV`
or `WindowsFastcall` when calling `extern "C"` functions or being called from C.

### Multi-return function

```rust
sig.returns.push(AbiParam::new(types::I64));  // return 0: value
sig.returns.push(AbiParam::new(types::I64));  // return 1: error code

// Caller retrieves:
let inst = builder.ins().call(fref, &args);
let results = builder.inst_results(inst);
let value = results[0];
let err   = results[1];
```

---

## 16. Function Calls

### Declare external function (import)

```rust
let mut sig = Signature::new(CallConv::SystemV);
sig.params.push(AbiParam::new(types::I64));
sig.returns.push(AbiParam::new(types::I64));

let func_id: FuncId = module.declare_function(
    "__MY_EXTERN_FN",
    Linkage::Import,
    &sig,
).unwrap();
```

### Import into current function and call

```rust
let fref = module.declare_func_in_func(func_id, builder.func);
let inst = builder.ins().call(fref, &[arg1, arg2]);
let results = builder.inst_results(inst);
let ret = results[0];
```

### Import deduplication — `declare_func_in_func` / `import_signature` do NOT dedup

`Module::declare_func_in_func` imports a **fresh** `SigRef` **and** `FuncRef` on
every call, even for a `FuncId` already imported in the same function. Its body is
literally `func.import_signature(decl.signature.clone())` + `import_function(...)`,
and `import_signature` just **pushes** onto `func.dfg.signatures` (no equality
check). So N call sites of the same callee emit N redundant `sigK =`/`fnK =`
preamble entries.

The duplicates are DCE'd from the final machine code, but they bloat the printed
IR and cost e-graph/verifier time at compile. A one-line `console.log` lowering
~600 runtime calls into one function produced **1947 decls for 229 distinct
callees / 9 distinct signatures** — collapsed to `229`/`9` by deduping. A
`FuncRef` is reusable across every `call` in the same function, so cache it.

**Dedup a FuncRef by FuncId (cheapest — a per-function cache):**

```rust
// Reset per function (a FuncRef is only valid inside the ir::Function it was
// imported into). Key by FuncId; the cached FuncRef reuses its SigRef too.
fn func_ref(cache: &mut HashMap<FuncId, FuncRef>,
            module: &mut dyn Module, builder: &mut FunctionBuilder,
            fid: FuncId) -> FuncRef {
    *cache.entry(fid).or_insert_with(|| declare_func_dedup(module, builder.func, fid))
}
```

**No per-function cache handy? Scan the already-imported `ext_funcs`** (scoped to
the builder, so it can never return a FuncRef from another function):

```rust
fn import_func(module: &mut dyn Module, builder: &mut FunctionBuilder,
               callee: FuncId) -> FuncRef {
    let want = callee.as_u32();
    for (fref, data) in builder.func.dfg.ext_funcs.iter() {
        if let ExternalName::User(nr) = data.name {
            let un = &builder.func.params.user_named_funcs()[nr];
            if un.namespace == 0 && un.index == want { return fref; }  // reuse
        }
    }
    declare_func_dedup(module, builder.func, callee)
}
```

**Also dedup the SigRef by content** (mirror `declare_func_in_func`, but reuse an
equal `Signature` — `ir::Signature` is `PartialEq`/`Eq`):

```rust
fn declare_func_dedup(module: &mut dyn Module, func: &mut ir::Function,
                      callee: FuncId) -> FuncRef {
    let decl = module.declarations().get_function_decl(callee);
    let want_sig  = decl.signature.clone();
    let colocated = decl.linkage.is_final();
    let sigref = func.dfg.signatures.iter()
        .find(|(_, s)| **s == want_sig).map(|(r, _)| r)
        .unwrap_or_else(|| func.import_signature(want_sig));
    let name_ref = func.declare_imported_user_function(
        UserExternalName::new(0, callee.as_u32()));
    func.import_function(ir::ExtFuncData {
        name: ExternalName::user(name_ref), signature: sigref,
        colocated, patchable: false,
    })
}
```

> RTS uses both paths: `Lowerer.func_ref` (HashMap by FuncId) for user-fn/thunk
> sites, `emit_marshal::import_func` (ext_funcs scan) at the runtime-call boundary,
> both landing in `declare_func_dedup`. See commit `f956f7af`.

### Indirect call (function pointer)

```rust
// sig must be declared in the current function
let sig_ref = builder.import_signature(sig.clone());

// fn_ptr is an I64 value holding the function address
let inst = builder.ins().call_indirect(sig_ref, fn_ptr, &[arg1]);
let ret = builder.inst_results(inst)[0];
```

### Tail call (frame reuse — eliminates stack growth)

```rust
// Only valid as the last instruction in a function
// Callee must have compatible signature
builder.ins().return_call(fref, &[arg1, arg2]);

// Indirect tail call
builder.ins().return_call_indirect(sig_ref, fn_ptr, &[arg1]);
```

**Requirements for tail calls on x86-64:**
- `preserve_frame_pointers = true` in ISA settings
- Function must use `CallConv::Tail`
- Must be the terminator of its block

---

## 17. Select (Branchless Conditional)

```rust
// if cond != 0 { true_val } else { false_val }
// No branch — maps to CMOV on x86, CSEL on aarch64
let result = builder.ins().select(cond, true_val, false_val);

// Bit select (mask): result[i] = if mask[i] { a[i] } else { b[i] }
let result = builder.ins().bitselect(mask, a, b);
```

Prefer `select` over `brif` when both values are cheap to compute and
the branch is unpredictable.

---

## 18. Specialized Arithmetic

### Wide multiply (128-bit result from 64-bit inputs)

```rust
// Returns the HIGH 64 bits of the full 128-bit product
let hi_s = builder.ins().smulhi(a, b);   // signed
let hi_u = builder.ins().umulhi(a, b);   // unsigned

// For full 128-bit result: low bits from imul, high bits from smulhi/umulhi
let lo = builder.ins().imul(a, b);
let hi = builder.ins().umulhi(a, b);
```

### Saturating arithmetic

```rust
// Clamp to type range instead of wrapping
let v = builder.ins().uadd_sat(a, b);    // unsigned saturating add
let v = builder.ins().sadd_sat(a, b);    // signed saturating add
let v = builder.ins().usub_sat(a, b);    // unsigned saturating sub
let v = builder.ins().ssub_sat(a, b);    // signed saturating sub

// Saturating multiply-round (DSP / fixed-point)
let v = builder.ins().sqmul_round_sat(a, b);
```

### Integer narrow / widen (for vector operations)

```rust
// Narrow: combine two N-bit vectors into one N/2-bit-element vector
let narrow_s = builder.ins().snarrow(lhs_vec, rhs_vec);   // signed, saturating
let narrow_u = builder.ins().unarrow(lhs_vec, rhs_vec);   // unsigned, saturating

// Widen: expand N-bit elements to 2N-bit elements
let wide_lo_s = builder.ins().swiden_low(vec);    // sign-extend low half
let wide_hi_s = builder.ins().swiden_high(vec);   // sign-extend high half
let wide_lo_u = builder.ins().uwiden_low(vec);    // zero-extend low half
let wide_hi_u = builder.ins().uwiden_high(vec);   // zero-extend high half
```

---

## 19. Atomic Operations

### Atomic load / store

```rust
use cranelift_codegen::ir::MemoryOrder;

let val = builder.ins().atomic_load(MemoryOrder::SeqCst, ptr);    // atomic read
builder.ins().atomic_store(MemoryOrder::Release, new_val, ptr);   // atomic write
```

### Atomic read-modify-write

```rust
use cranelift_codegen::ir::AtomicRmwOp;

// Returns OLD value before the operation
let old = builder.ins().atomic_rmw(AtomicRmwOp::Add,  MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Sub,  MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::And,  MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Or,   MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Xor,  MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Nand, MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Xchg, MemoryOrder::SeqCst, ptr, val); // exchange
let old = builder.ins().atomic_rmw(AtomicRmwOp::Umin, MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Umax, MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Smin, MemoryOrder::SeqCst, ptr, val);
let old = builder.ins().atomic_rmw(AtomicRmwOp::Smax, MemoryOrder::SeqCst, ptr, val);
```

### Atomic compare-and-swap (CAS)

```rust
// Returns the actual old value (compare to expected to check if swap happened)
let actual = builder.ins().atomic_cas(
    MemoryOrder::SeqCst,
    ptr,         // address
    expected,    // expected old value
    replacement, // new value to write if expected matches
);
let swapped = builder.ins().icmp(IntCC::Equal, actual, expected);
```

### Memory fence

```rust
builder.ins().fence(MemoryOrder::SeqCst);     // full barrier
builder.ins().fence(MemoryOrder::Release);    // store fence
builder.ins().fence(MemoryOrder::Acquire);    // load fence
```

### Memory orderings

| `MemoryOrder` | C++ equivalent | Meaning |
|---|---|---|
| `Relaxed`  | `memory_order_relaxed` | No synchronization, no ordering |
| `Acquire`  | `memory_order_acquire` | Load — all subsequent reads/writes see prior releases |
| `Release`  | `memory_order_release` | Store — prior writes visible to subsequent acquires |
| `AcqRel`   | `memory_order_acq_rel` | Both acquire and release (for RMW) |
| `SeqCst`   | `memory_order_seq_cst` | Total sequential order across all threads |

---

## 20. SIMD / Vector Operations

### Build a vector value

```rust
// Splat scalar to all lanes: [x, x, x, x]
let v = builder.ins().splat(types::I32X4, scalar_i32);

// Build lane-by-lane
let zero = builder.ins().iconst(types::I32, 0);
let mut v = builder.ins().scalar_to_vector(types::I32X4, lane0);
v = builder.ins().insertlane(v, 1, lane1);
v = builder.ins().insertlane(v, 2, lane2);
v = builder.ins().insertlane(v, 3, lane3);

// Load from memory (aligned)
let v = builder.ins().load(types::I32X4, MemFlags::trusted().with_aligned(), ptr, 0);
```

### Lane extraction

```rust
let scalar: Value = builder.ins().extractlane(vector, 2);  // lane index = immediate
```

### Shuffle (compile-time lane permutation)

```rust
// lanes from [lhs | rhs] by index (0..15 for I8X16)
use cranelift_codegen::ir::immediates::Uimm128;
let mask = Uimm128::from([0,1,2,3, 8,9,10,11, 4,5,6,7, 12,13,14,15]);
let out = builder.ins().shuffle(lhs, rhs, mask);
```

### Swizzle (runtime lane permutation)

```rust
// indices is an I8X16 vector of lane indices (dynamic)
let out = builder.ins().swizzle(vector, indices);
```

### Reductions

```rust
let any  = builder.ins().vany_true(vec);    // 1 if any lane != 0
let all  = builder.ins().vall_true(vec);    // 1 if all lanes != 0
let bits = builder.ins().vhigh_bits(vec);   // bitmask of sign bits (one per lane)
```

### Vector arithmetic

Vector types use the same arithmetic instructions as scalars — Cranelift
applies them lane-wise automatically:

```rust
// All of these work on vector types too:
let v = builder.ins().iadd(va, vb);         // I32X4 + I32X4
let v = builder.ins().fadd(va, vb);         // F32X4 + F32X4
let v = builder.ins().imul(va, vb);         // I32X4 * I32X4
let v = builder.ins().fmul(va, vb);         // F64X2 * F64X2
let v = builder.ins().icmp(IntCC::Equal, va, vb); // lane-wise compare
```

---

## 21. GC Stack Maps

Cranelift provides precise stack maps that allow a GC to find live GC-managed
values on the stack at every potential collection point (every call site).

### Mark a value as needing a stack map entry

```rust
// During codegen, after producing a GC handle value:
builder.declare_value_needs_stack_map(gc_handle_value);
```

### Extract stack map after compilation

```rust
use cranelift_codegen::CompiledCode;

// After module.define_function(func_id, &mut ctx):
let compiled: &CompiledCode = ctx.compiled_code().unwrap();
let stack_maps: &[MachStackMap] = compiled.buffer.stack_maps();

for sm in stack_maps {
    // sm.offset_end = offset from function start where this map is valid
    // sm.map = bitset of stack slots holding live GC values
}
```

### JIT: map function-relative offsets to absolute addresses

```rust
// After module.finalize_definitions():
let fn_ptr = module.get_finalized_function(func_id);
let fn_base = fn_ptr as usize;

for sm in &stack_maps {
    let call_site_pc = fn_base + sm.offset_end as usize;
    gc_registry.register(call_site_pc, &sm.map);
}
```

---

## 22. Optimization Settings

All set via `settings::builder()` before creating the ISA:

```rust
let mut b = settings::builder();

// Optimization level
b.set("opt_level", "speed").unwrap();          // "none" | "speed" | "speed_and_size"

// E-graph based optimizer (best overall improvement, recommended on)
b.set("use_egraphs", "true").unwrap();

// Alias analysis (enables load/store motion)
b.set("enable_alias_analysis", "true").unwrap();

// Jump tables (enables br_table instead of branch chains)
b.set("enable_jump_tables", "true").unwrap();

// Frame pointers (required for tail calls and stack walking on x86-64)
b.set("preserve_frame_pointers", "true").unwrap();

// Position-independent code (for shared libraries / ASLR)
b.set("is_pic", "true").unwrap();

// Unwind info (for exception handling integration)
b.set("unwind_info", "true").unwrap();

// Probestack (avoid stack clash attacks, required on some OS)
b.set("enable_probestack", "false").unwrap();

let flags = settings::Flags::new(b);
```

### What the optimizer does (and doesn't do)

| Optimization | Cranelift does it | Notes |
|---|---|---|
| Constant folding | Yes (e-graphs) | `iconst` arithmetic folded at compile time |
| Dead code elimination | Yes | Unused values removed |
| Common subexpression elimination | Yes (e-graphs) | |
| Register allocation | Yes | Linear scan |
| Instruction selection | Yes | Target-native instructions |
| Peephole patterns | Yes | `iadd_imm`, `ishl_imm`, etc. |
| Loop invariant code motion | Limited | |
| Inlining | **No** | Must be done before emitting IR |
| Autovectorization | **No** | Must emit vector types explicitly |
| Global value numbering | Partial (e-graphs) | |
| Escape analysis | **No** | Must be done in your IR layer |
| Alias analysis across calls | **No** | Stops at call boundaries |

For optimizations Cranelift doesn't do, implement them in your HIR/MIR layer
before lowering to Cranelift IR.

---

## 23. Common Patterns

### If / else

```rust
let then_block = builder.create_block();
let else_block = builder.create_block();
let merge      = builder.create_block();
let result_param = builder.append_block_param(merge, types::I64);

builder.ins().brif(cond, then_block, &[], else_block, &[]);

builder.switch_to_block(then_block);
builder.seal_block(then_block);
let then_val = /* ... */;
builder.ins().jump(merge, &[then_val]);

builder.switch_to_block(else_block);
builder.seal_block(else_block);
let else_val = /* ... */;
builder.ins().jump(merge, &[else_val]);

builder.switch_to_block(merge);
builder.seal_block(merge);
// result_param holds the value from whichever branch executed
```

### While loop

```rust
let header = builder.create_block();
let body   = builder.create_block();
let exit   = builder.create_block();

builder.ins().jump(header, &[]);

builder.switch_to_block(header);
let cond = /* evaluate condition */;
builder.ins().brif(cond, body, &[], exit, &[]);
builder.seal_block(header);

builder.switch_to_block(body);
builder.seal_block(body);
/* loop body */
builder.ins().jump(header, &[]);

builder.switch_to_block(exit);
builder.seal_block(exit);
```

### For loop with counter (Variable approach)

```rust
let i_var = Variable::new(0);
builder.declare_var(i_var, types::I64);
let zero = builder.ins().iconst(types::I64, 0);
builder.def_var(i_var, zero);

let header = builder.create_block();
let body   = builder.create_block();
let exit   = builder.create_block();

builder.ins().jump(header, &[]);

builder.switch_to_block(header);
let i = builder.use_var(i_var);
let limit = builder.ins().iconst(types::I64, n as i64);
let cond = builder.ins().icmp(IntCC::SignedLessThan, i, limit);
builder.ins().brif(cond, body, &[], exit, &[]);
builder.seal_block(header);

builder.switch_to_block(body);
builder.seal_block(body);
/* loop body uses i */
let next_i = builder.ins().iadd_imm(i, 1);
builder.def_var(i_var, next_i);
builder.ins().jump(header, &[]);

builder.switch_to_block(exit);
builder.seal_block(exit);
```

### Call external `extern "C"` function

```rust
fn get_or_declare_extern(
    module: &mut impl Module,
    func: &mut cranelift_codegen::ir::Function,
    name: &str,
    params: &[Type],
    ret: Option<Type>,
) -> FuncRef {
    let mut sig = Signature::new(CallConv::SystemV);
    for &p in params { sig.params.push(AbiParam::new(p)); }
    if let Some(r) = ret { sig.returns.push(AbiParam::new(r)); }
    let fid = module.declare_function(name, Linkage::Import, &sig).unwrap();
    module.declare_func_in_func(fid, func)
}

// Usage:
let fref = get_or_declare_extern(&mut module, builder.func,
    "__MY_RT_FN", &[types::I64, types::I64], Some(types::I64));
let inst = builder.ins().call(fref, &[a, b]);
let result = builder.inst_results(inst)[0];
```

### Struct field access via pointer + offset

```rust
// Given base_ptr pointing to a struct:
// { field0: i64 @ offset 0, field1: f64 @ offset 8, field2: i32 @ offset 16 }
let field0 = builder.ins().load(types::I64, MemFlags::trusted(), base_ptr, 0);
let field1 = builder.ins().load(types::F64, MemFlags::trusted(), base_ptr, 8);
let field2 = builder.ins().sload32(MemFlags::trusted(), base_ptr, 16); // sign-extend i32 → i64

// Write:
builder.ins().store(MemFlags::trusted(), new_val, base_ptr, 0);
builder.ins().istore32(MemFlags::trusted(), i32_val, base_ptr, 16);
```

### Array element access

```rust
// array[index] where element_size = 8 (i64)
let element_size = builder.ins().iconst(types::I64, 8);
let byte_offset  = builder.ins().imul(index, element_size);
let elem_ptr     = builder.ins().iadd(array_base, byte_offset);
let elem         = builder.ins().load(types::I64, MemFlags::new(), elem_ptr, 0);

// With known element size (strength-reduced):
let byte_offset = builder.ins().ishl_imm(index, 3);  // index * 8 = index << 3
let elem_ptr    = builder.ins().iadd(array_base, byte_offset);
```

### 64-bit integer as low-level type storage

When building a typed language that needs to store i8/i16/i32 in a uniform
storage cell (e.g., tagged value or union), use bitmasking:

```rust
// Store i8 value in low byte of i64
let masked = builder.ins().band_imm(value_i64, 0xFF);
// Read back as signed i8
let as_i8  = builder.ins().ireduce(types::I8, value_i64);
let as_i64_s = builder.ins().sextend(types::I64, as_i8);  // sign-extend back

// Store u16 value
let masked = builder.ins().band_imm(value_i64, 0xFFFF);

// Read as signed i16 → i64
let as_i16   = builder.ins().ireduce(types::I16, value_i64);
let as_i64_s = builder.ins().sextend(types::I64, as_i16);
```

---

## 24. Pitfalls and Rules

### Types must match
Binary instructions (`iadd`, `fadd`, etc.) require both operands to have the
**same type**. Mixing I32 and I64 without conversion is a verifier error.
Always emit explicit `sextend`/`uextend`/`ireduce` before mixing.

```rust
// WRONG: mixing I32 and I64
builder.ins().iadd(i32_val, i64_val);

// RIGHT:
let wide = builder.ins().sextend(types::I64, i32_val);
builder.ins().iadd(wide, i64_val);
```

### Seal every block exactly once
Every block must be sealed after all its predecessors have been wired. Sealing
a block twice, or forgetting to seal, produces incorrect phi nodes.

```rust
// Pattern: seal immediately if no back-edges exist
builder.switch_to_block(block);
builder.seal_block(block);  // OK: sealed right after switch for blocks with no back-edge

// Loop header: seal AFTER the back-edge jump is emitted
builder.switch_to_block(body);
/* ... body ... */
builder.ins().jump(header, &[]);  // back-edge
builder.seal_block(header);       // NOW seal — all predecessors known
```

### Integer division does not trap automatically
`sdiv` and `udiv` will trap on division-by-zero on most targets, but this
behavior is target-dependent. Emit an explicit guard if your language semantics
require a defined error (rather than UB):

```rust
let is_zero = builder.ins().icmp_imm(IntCC::Equal, divisor, 0);
builder.ins().trapnz(is_zero, TrapCode::IntegerDivisionByZero);
let result = builder.ins().sdiv(dividend, divisor);
```

### `icmp` returns I8, not I64
`icmp` and `fcmp` produce an `I8` (0 or 1). For `brif`, this is fine — it
accepts any integer. For use as an I64 value in your runtime (e.g., as a bool
ABI return), extend it:

```rust
let cmp = builder.ins().icmp(IntCC::Equal, a, b);  // I8
let as_i64 = builder.ins().uextend(types::I64, cmp); // I8 → I64 (0 or 1)
```

### Tail calls require matching signature and `CallConv::Tail`
A `return_call` is only valid when:
1. The current function uses `CallConv::Tail`
2. `preserve_frame_pointers = true` is set in ISA settings (x86-64)
3. The callee's return types match the current function's return types

### `MemFlags::trusted()` vs `MemFlags::notrap()`
- `trusted()` = the pointer is **always valid and aligned**; the optimizer may
  hoist the load out of loops. Use for stack slots and known globals only.
- `notrap()` = the load will not trap even if the pointer is invalid (speculative
  execution safe). Use only when you've validated the pointer at a language level.
- Default `MemFlags::new()` = may trap (safe default for unknown pointers).

### Block parameters are not function parameters
`append_block_param` adds a phi-like parameter to a block — it receives its
value from the `args` list in any `jump` or `brif` that targets the block.
Do not confuse with `append_block_params_for_function_params`, which wires
the entry block's params to the function's ABI parameters.

### `use_egraphs` is the main optimizer
Without `use_egraphs = true`, Cranelift does very little optimization. Enable
it in production. The e-graph pass handles constant folding, CSE, and algebraic
identities (e.g., `x + 0 → x`, `x * 1 → x`, `x & 0 → 0`).

### I128 has limited backend support
`types::I128` is supported for basic arithmetic and loads/stores, but some
backends decompose it into two 64-bit operations. Avoid I128 in hot paths;
use `smulhi`/`umulhi` + `imul` instead for 128-bit products.

### Function/signature imports are not deduplicated
`declare_func_in_func` and `import_signature` push a new `FuncRef`/`SigRef` every
call — importing the same callee from 100 sites yields 100 near-identical preamble
decls. Cache a `FuncRef` per `FuncId` for the current function (and reuse an equal
`Signature`) to keep the preamble to one entry per distinct callee/shape. See
§16 "Import deduplication" for the pattern.

### `fcvt_to_sint` TRAPS on NaN / out-of-range — JS needs the `_sat` form
The trapping `fcvt_to_sint` / `fcvt_to_uint` raise a `BadConversionToInteger`
(NaN) or `IntegerOverflow` trap for any float outside the integer's range — on
x86-64 a raw `cvttsd2si` of an out-of-range double, which at runtime surfaces as a
**SIGILL / illegal-instruction** through Cranelift's trap handler, not a clean
error. JS `ToInt32`/`ToUint32` never trap (NaN/±∞ → 0, otherwise truncate-then-
wrap). When lowering a JS number → int coercion always use the **saturating**
`fcvt_to_sint_sat` / `fcvt_to_uint_sat` (clamp, no trap), then apply the JS wrap
separately if `ToInt32` semantics are required. RTS shipped exactly this bug: a
`coerce(Tagged→Int)` used the trapping form and crashed ~19 fixtures with SIGILL
until switched to `_sat`.

---

## 25. Exception Handling — `try_call` and exception tables

Cranelift 0.131 has a native exception mechanism (distinct from Rust panics /
`std::arch` unwinding): `try_call` / `try_call_indirect` call a function while
routing an unwound exception to a handler block via an **exception table**.

- `ir::ExceptionTable` (entity `"extable"`) + `ExceptionTableData::new(sig,
  normal_return: BlockCall, matches)` — `matches` is a set of
  `ExceptionTableItem::{Tag(ExceptionTag, BlockCall), Default(BlockCall),
  Context(..)}`; `normal_return` is the no-exception successor.
- The exceptional edge receives payload values through the special block args
  `BlockArg::TryCallRet(i)` (a normal return value on the fallthrough edge) and
  `BlockArg::TryCallExn(i)` (an exception payload on a catch edge) — you don't
  pass these as ordinary `Value`s; they are generated "on the edge" out of the
  `try_call`.
- `try_call` must be a **block terminator** (like `brif`), because it has
  successor edges (normal + handler).

This is the substrate for *real* zero-cost unwinding. A runtime that instead uses
a thread-local error slot + explicit post-call checks (RTS's current model, #128
phase 1) does NOT need exception tables; migrating to `try_call` is what a "real
unwind" phase would build on. Documented here as the available primitive — verify
the exact generated `InstBuilder::try_call` argument order against your Cranelift
version before wiring it, since the opcode is meta-generated.

---

## 26. Libcalls — runtime routines the backend may emit

Some IR lowers to a call into a small fixed set of runtime routines rather than
inline instructions. The `ir::LibCall` enum enumerates them; the JIT/AOT must
provide their symbols (the default `cranelift_module::default_libcall_names` maps
them to the usual C names).

- Float rounding/FMA when no native instruction: `CeilF32/64`, `FloorF32/64`,
  `TruncF32/64`, `NearestF32/64`, `FmaF32/64`.
- Bulk memory: `Memcpy`, `Memset`, `Memmove`, `Memcmp` (large `stack_load`/
  aggregate copies may lower to these).
- Stack-overflow probing: `Probestack` (emitted when `enable_probestack` is on).
- TLS access: `ElfTlsGetAddr`, `ElfTlsGetOffset`.
- Fallback SIMD: `X86Pshufb` (shuffle when SSSE3 is absent).

For a **JIT**, register these with `JITBuilder::symbol`/`symbols` (or rely on the
default libcall names resolving through the process). For **AOT**, they become
relocations resolved by the system linker against libc. A missing libcall symbol
is a link-time error, not a codegen error.

---

## 27. Cold Blocks and Layout Hints

Mark rarely-taken blocks (error paths, slow-path IC misses, deopt stubs) as COLD
so the register allocator and block layout push them out of the hot path (out of
line, spills preferred there over the fast path):

```rust
// After creating the block, before/while filling it:
builder.func.layout.set_cold(slow_block);
let is_cold = builder.func.layout.is_cold(slow_block);
```

Cold blocks are placed after the function body, keep the hot path's I-cache
dense, and bias spill/reload decisions away from the common case. Pair with a
`brif` whose likely edge is the hot block. This is a hint, not a guarantee — the
backend is free to ignore it, and correctness never depends on it.

---

*End of reference. Covers Cranelift 0.131.0 API surface.*
