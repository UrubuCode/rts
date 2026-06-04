# Codegen optimizations + artifact layout + docs

> Note: the codegen paths below live in
> `crates/rts-codegen/src/codegen/lower/` (authoritative AST path) and
> `crates/rts-codegen/src/codegen/mir_codegen/` (MIR layer, default ON). HIR +
> MIR are in production since Phase 3 of `RTS_REFACTOR.md` (commits
> f7b924b/23dd4b7).

## MIR layer (`mir_codegen/`) — parallel to AST codegen

`crates/rts-codegen/src/codegen/mir_codegen/` consumes `MirFunc` from the
`rts-mir` crate and emits Cranelift IR via `FunctionBuilder` 1:1. `hint_bridge`
converts `CraneliftTypeHint` to `cl::Type`; `lower.rs` translates
`Inst`/`Terminator`; `extern_resolver_default()` resolves RTS namespaces via
`crate::abi::SPECS`.

Hybrid routing in `compile_user_fn`: each user fn tries MIR; silent bail to AST
when it hits an unmodeled construct (member on this/objects, classes,
async/await, address-taken fns, string in user-fn params/ret). Default ON;
`RTS_USE_MIR=0` disables, `RTS_USE_MIR=fn1,fn2,...` restricts.

MIR-level optimizations. `optimize()` runs in order: **fold → fma → cse → dce**.
Plus `inline` in fixed-point (max 4 iterations with `optimize` between passes)
between lower and optimize in `try_compile_via_mir`.

- `passes/fold.rs` — constant folding (IAdd/ISub/IMul/SDiv/SRem/BAnd/BOr/BXor of
  IConst→IConst) + strength reduction (mul→shl, urem→band, sdiv→sshr, ops with
  const→`*Imm`)
- `passes/fma.rs` — FMA fusion `a*b+c → Fma`, conservative (only fuses when the
  `FMul` has 1 use, avoiding duplicated work) (step 4.8)
- `passes/cse.rs` — intra-block Common Subexpression Elimination (step 4.5)
- `passes/dce.rs` — dead-code elimination with fixed-point (preserves
  side-effecting: Store, CallExtern, AtomicStore/Rmw/Cas, Fence, DeclareGcValue)
- `passes/inline.rs` — inlining of small fns, `INLINE_BUDGET=16`, conservative
  eligibility (no recursion); run in fixed-point up to 4 iters via thread-local
  `MIR_CACHE` + pre-registration of HIR signatures (steps 4.2/4.3/4.7)
- `passes/narrow.rs` — I8/U8 (mask 0xFF) and I16/U16 (mask 0xFFFF)
  canonicalization after IAdd/ISub/IMul/INeg/IShl
- `passes/verify.rs` — invariants (block ids match position, ValueIds in range,
  consistent params count)
- Intrinsic inlining: the `Intrinsic` tag on the namespace spec generates a
  specialized Inst (Sqrt, FAbs, FMin/FMax, IAbs, IMin/IMax) instead of
  `CallExtern`; `mir_codegen` lowers directly to native Cranelift IR.
- Atomics in `mir_codegen` (step 4.1): `Inst::AtomicLoad`/`AtomicStore`/
  `AtomicRmw`/`AtomicCas`/`Fence` lower directly to Cranelift's `atomic_*` with
  `MemOrder`/`RmwOp` mapping.

## Notable codegen optimizations

- **Inline intrinsics** (`abi::Intrinsic`): `sqrt`, `abs_f64`, `min/max_f64`,
  `abs_i64`, `min/max_i64`, `random_f64` — emitted as direct Cranelift IR in
  `lower_intrinsic`
- **Tail call optimization**: user functions in `CallConv::Tail`; `return f(x)`
  in tail position emits `return_call` (requires `preserve_frame_pointers=true`
  on x86-64)
- **First-class function pointers** (#97 phase 1): `Expr::Ident` resolving to a
  user fn materializes `func_addr` as i64; call via local/param ident does
  `call_indirect` with a provisional Tail signature
- **Jump table switch**: when all non-default cases are integer literals, uses
  `cranelift_frontend::Switch` (the backend decides `br_table` vs binary search)
- **Imm forms**: `x + N` / `x & MASK` / `x << K` emit `iadd_imm` / `band_imm` /
  `ishl_imm` without an intermediate iconst
- **MemFlags::trusted** on global and RNG-state loads/stores
- **f64 modulo** via libc `fmod` (previously truncated via i64, losing the
  fractional part)
- **Constants as properties** (`math.PI` without parens) via
  `MemberKind::Constant` + `emit_constant_load`
- **Function class (#359)**: the `invoke_n` trampoline dispatches by arity up to
  8 via transmute to `extern "C" fn(i64...) -> i64`. Reify of a user-fn ident
  into a Function handle only on member access (direct calls still use the fast
  `call_indirect`).
- **expand_async_functions (#437)**: a simplified post-refactor pass emits `f =
  (args) => promise.create(__async_inner_f, args)` instead of the old synthetic
  wrapper (~110 LOC less per async fn).

## Inline assembly (`std::arch::asm!`) — available technique

When the problem requires ABI/register control that safe Rust can't express
(calling a `fn_ptr` with dynamic arity, reading RSP/registers, manipulating the
call frame), **inline assembly via `std::arch::asm!` is a legitimate technique
already used in the project** — not a forbidden last resort. Live cases:

- **`gc/collector.rs`** — `asm!("mov {}, rsp", ...)` captures the stack pointer
  in the root scanner.
- **`globals/function/ops.rs::invoke_all_i64`** (#1281) — Win64 trampoline that
  assembles args dynamically (4 in RCX/RDX/R8/R9, the rest on the stack with 32
  shadow space + 16 alignment before the `call`). Replaced an arity-≤8 `match`
  (wrong result / ACCESS_VIOLATION above the cap) with **variable arity N** with
  no artificial limit.

Rules when using inline asm:

- **Always `#[cfg(...)]` per target** + **portable fallback** (`#[cfg(not(...))]`)
  — don't break CI/builds on other platforms.
- **List all clobbers** (caller-saved GP + XMM). NB: `clobber_abi("win64")`
  conflicts with explicit `out("rax")` — use one form or the other.
- **Respect the target ABI** (Win64: 4 register args + 32 shadow space + stack
  16-aligned before the `call`).
- **Document the assumed convention** in a doc-comment.
- The explicit-regression rule applies: run `cargo test --release --lib` +
  `rts.exe test` after changing asm; any regression must be known and justified,
  never silent.

Use it when the safe alternative would be an artificial limit or impossible
(reading registers). For common logic, prefer Cranelift IR / Rust.

## Pending optimizations / backlog

See open issues #90, #96, #97 (phases 2/3). #92 autovec was closed as infeasible
without our own loop vectorizer (Cranelift has none); Bun wins on Monte Carlo >1B
iter via V8 autovec.

## User artifact layout

Phase 1 roadmap target (in progress):

```
<project>/
  src/main.ts
  package.json
  tsconfig.json

  node_modules/.rts/
    objs/
      runtime/        — full builtin objects (all modules)
      compile/        — AOT objects with slicing (only on rts compile)
    modules/          — resolved + cached modules (with .ometa metadata)

  release/            — only on rts compile
    <project_name>    — .exe / .dll / .so / .node per target
```

## Docs and specs

The `docs/specs/` folder holds feature specs, design decisions, and technical
notes. See the index at `docs/specs/INDEX.md`. High-level direction lives in
`RTS_REFACTOR.md` at the root (canonical plan for the crate-workspace refactor).

Relevant active specs:
- `docs/specs/namespace-creation-guide.md` — current process based on
  `crates/rts-abi/`
- `docs/specs/silent-parallelism.md` — pipeline of the 3 passes
- `docs/specs/async-promise-function.md` — unified async/Promise/Function system
  (#359 + #437)
