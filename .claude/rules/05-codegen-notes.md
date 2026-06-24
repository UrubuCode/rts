# Codegen notes (engine) + artifact layout + docs

> Canonical design: `docs/specs/rts-codegen-new-design.md`. The engine's lowering
> lives in `crates/rts-codegen-new/src/front/run/` (single HIR → Cranelift path);
> the value model is in `crates/rts-adapters/`. There is no MIR tier and no dual
> AST path.

## One optimizer tier — the Cranelift egraph

The engine has **no second optimizer tier**. The single lowering path
`HIR → Cranelift IR` feeds the Cranelift egraph (`use_egraphs=true`), which is the
**sole** optimizer: const-fold, CSE, DCE, FMA, strength reduction, intraprocedural
inlining (the deleted MIR tier re-did exactly what the egraph already does).

The front-end's only job is what Cranelift genuinely cannot do (JS semantics):
`ToNumber`/`ToString`/`ToBoolean` coercions, the polymorphic `+` resolution,
box/unbox insertion, shape/IC site emission, narrow-int (i8/u8/i16/u16) wrap
semantics, and exception edges. Everything else is the
egraph's.

### box/unbox as pure Cranelift IR (the key coupling)

PolyValue box/unbox are `bitcast` / `band` / `bor` / `icmp` / `select` —
**pure IR, never extern calls**. Because they are pure, the egraph **folds a
redundant `box(unbox(x))` pair**, so the PolyValue cost vanishes exactly where
the representation was already monomorphic. This is the technical reason Pilar 1
(PolyValue) and Pilar 5 (single lowering) are coupled (design doc §9.3): if
box/unbox were extern calls the egraph could not see through them and the cost
would survive.

## Notable codegen optimizations (preserved / new)

- **Inline intrinsics** (`abi::Intrinsic`): `sqrt`, `abs_f64`, `min/max_f64`,
  `abs_i64`, `min/max_i64`, `random_f64` — emitted as direct Cranelift IR.
  **Preserved**; the intrinsic spec tag still inlines to native IR (design doc
  §3.1 / §10.1).
- **Polymorphic `+` fast path**: both-proven-number → native `iadd`/`fadd` (zero
  cost); otherwise ONE `ADD_GENERIC` with an inline tag-check fast path for the
  secretly-monomorphic case. No AST-shape guessing.
- **Shapes + data ICs**: property access = shape-id compare + fixed-offset load;
  method dispatch = shape-keyed (not O(N) `gc.string_eq`). `PropIcCell` data cell,
  `uninit → mono → poly → mega` (design doc §8).
- **Tail call optimization**: user functions in `CallConv::Tail`; `return f(x)`
  in tail position emits `return_call` (requires `preserve_frame_pointers=true`
  on x86-64).
- **First-class function pointers**: an ident resolving to a user fn materializes
  `func_addr`; call via local/param ident does `call_indirect`.
- **Imm forms / MemFlags::trusted / f64 mod via libc fmod / constants as
  properties** — the front-end emits these; the egraph cleans up.
- **Data-driven dispatch + harvested ABI**: every non-primordial method is a
  `MethodSpec` resolved by one `resolve_method` path; the JIT symbol table is
  harvested from Registry fn-ptrs in `crates/rts-codegen-new/src/adapter_symbols/`
  (drift/coverage guard) — killing the link-OK/runtime-SIGILL class of bug (design
  doc §10).

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
notes. See the index at `docs/specs/INDEX.md`. **The canonical engine direction
is `docs/specs/rts-codegen-new-design.md`** (the ground-up redesign plan; read it
before any engine work).

Relevant active specs:
- `docs/specs/rts-codegen-new-design.md` — canonical engine redesign (PolyValue,
  Repr lattice, shapes + data ICs, single lowering, data-driven dispatch)
- `docs/specs/namespace-creation-guide.md` — namespace process based on
  `rts-engine::abi`
- `docs/specs/gc-generational-design.md` — GC: weak phase now (#217, bounded),
  generational copying nursery later (deferred until ~90% cross-runtime)
- `docs/specs/async-promise-function.md` — async/Promise/Function system
  (#359 + #437; the new engine's interim async is SYNCHRONOUS)
