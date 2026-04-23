# RTS — Next Steps

Current state (branch `feat/remake-namespaces`): legacy dispatch/`RuntimeValue`/MIR/HIR fully excised. New ABI (`src/abi/`), codegen (`src/codegen/`), pipeline (`src/pipeline.rs`), and embedded runtime (`src/runtime/embedded.rs` + `src/main.rs`) deliver end-to-end compile for **top-level namespace calls with literal arguments only**.

Namespaces implemented on the new ABI: `gc`, `io`, `fs`.
Legacy namespaces removed: `buffer`, `crypto`, `globals`, `json`, `net`, `process`, `promise`, `rust`, `str`, `task`, `test`.

`rts compile hello.ts` produces a native binary that links against the embedded RTS static library (auto-discovered MSVC toolchain on Windows).

Benchmark runs without errors, but the compiled binary is a no-op — the source script uses user-defined functions, loops, variables, arithmetic, and string concat, none of which the bootstrap codegen supports. Comparison against Bun/Node is not meaningful until codegen is expanded.

---

## Phase A — Codegen expansion

Goal: compile the full bench (`bench/rts_simple.ts`) with real semantics.

### A1. Local variables + type annotations
- [ ] Parse `let x: i32 = 0`, `const LCG_MOD: i32 = 2147483647` from SWC `Stmt::Decl(Var(...))`.
- [ ] Reserve Cranelift `Variable`s keyed by name within function scope.
- [ ] Respect `i32` / `i64` / `f64` / `bool` / `str` annotations; default unannotated numerics to `i64`.
- [ ] Warning → error when an untyped binding is mutated across branches with mismatched types (defer type inference to Phase B).

### A2. Arithmetic + comparison + modulo
- [ ] Lower `BinaryOp` for `+`, `-`, `*`, `/`, `%`, `===`, `!==`, `<`, `<=`, `>`, `>=`, `&&`, `||`.
- [ ] Cranelift primitives: `iadd`, `isub`, `imul`, `sdiv`, `srem`, `icmp`, `band`, `bor`, `fadd`, `fsub`, `fmul`, `fdiv`.
- [ ] Numeric literal → IR const via `iconst`/`f64const`.
- [ ] String concat `"a" + x` = out of scope in this phase; requires runtime helper `__RTS_FN_NS_GC_STRING_CONCAT_FROM_PARTS`.

### A3. Control flow
- [ ] `if` / `else` → Cranelift blocks + `brif`.
- [ ] `while` / `do-while` → header block + cond + body block.
- [ ] `switch` / `case` / `default` / `break` → jump table via `Switch::build` or cascaded `brif`.
- [ ] `return` → `return_` with expected type.

### A4. User-defined functions
- [ ] Declare Cranelift function per `FunctionDecl` (module-level first, nested later).
- [ ] Signature from parameter type annotations + return annotation.
- [ ] Function body uses the lowerers from A1-A3.
- [ ] Function calls resolved by name; mismatched arity / types produce diagnostics at call site.

### A5. String values
- [ ] Implement `__RTS_FN_NS_GC_STRING_FROM_I64(value: i64) -> handle` and `_FROM_F64`.
- [ ] Implement `__RTS_FN_NS_GC_STRING_CONCAT(a_handle, b_handle) -> handle`.
- [ ] Codegen for `a + b` where at least one side is a string handle → lower to concat call chain, produce handle.
- [ ] `io.print(handle)` path: add overload or coerce handle → `StrPtr` via `STRING_PTR` + `STRING_LEN`.

### A6. Top-level bindings as globals
- [ ] `let` / `const` at module scope → Cranelift data symbol + load/store in `main`.
- [ ] Respect initialiser: evaluate at program start before any user call.

### A7. Regression test suite
- [ ] `cargo test` target: fixtures under `tests/fixtures/*.ts` + expected stdout.
- [ ] Each fixture compiled and executed; compare stdout to `<name>.out`.
- [ ] Enumerate cases: empty program, single print, if/else, while loop, function call, string concat, arithmetic stress.

---

## Phase B — Type system integration

Goal: real type checking before codegen; emit precise diagnostics instead of "warning: unsupported".

### B1. Re-attach type checker to new pipeline
- [ ] `src/type_system/checker.rs` already exists; re-wire it after parse, before codegen.
- [ ] Resolve identifier references against namespace SPECs + user decls.
- [ ] Reject unknown identifiers, wrong arity, wrong literal types with `DiagnosticEngine`.

### B2. Signature specialisation
- [ ] Integer literal `3` typed as `i32` in an `i32` context, `i64` otherwise.
- [ ] Codegen consumes resolved types rather than inferring from literal shape.

### B3. Strict mode
- [ ] Flag `--strict` turns implicit coercions into errors.
- [ ] Default mode keeps bench-compatible permissive behaviour.

---

## Phase C — Remaining namespaces

Rebuild each on the new ABI (`SPEC` + `__RTS_FN_NS_*` per member). Priority order:

1. [ ] **`crypto`** — sha256 (already had `_direct` template), sha1, hmac-sha256.
2. [ ] **`process`** — `exit`, `argv`, `env_get`, `env_set`, `cwd`.
3. [ ] **`str`** — length, slice, concat, index_of, chars, trim. Operates on GC handles.
4. [ ] **`net/tcp`** — listen, accept, connect, send, recv, close. Handles via GC.
5. [ ] **`net/udp`** — send_to, recv_from.
6. [ ] **`globals`** — get/set/has/delete over a GC-rooted map.
7. [ ] **`buffer`** — new, read, write, slice, copy; byte-level `Vec<u8>` backing.
8. [ ] **`promise`** — depends on async runtime, see Phase E.
9. [ ] **`task`** — depends on async runtime, see Phase E.
10. [ ] **`test`** — thin helpers over `io`; mostly stdout assertions.

Each namespace ships:
- `abi.rs` with `SPEC` + `MEMBERS`
- One file per member group (e.g. `tcp.rs`, `udp.rs`)
- Unit tests covering happy path + error code returns
- Registration in `src/abi/mod.rs` `SPECS` array

---

## Phase D — GC proper

Slab-based handle table is a bootstrap. Replace with:

- [ ] `gc-arena` integration; `Arena::mutate` around user code boundaries. *(crate já em Cargo.toml)*
- [ ] Roots: globals map, thread-local binding table, currently-executing frame locals.
- [ ] `safe_collect()` hook points: after function return, after method body, after closure scope.
- [ ] `__RTS_FN_NS_GC_COLLECT` extern to force-run a cycle for benchmarks.
- [ ] String pool → interning for short strings (< 24 bytes inline in `RtsValue` payload).

Deliverable: allocator stress test with no leaks (`rts test` fixture that allocates 1M strings, asserts `rss < N MB`).

---

## Phase E — Async runtime

- [ ] `task::spawn`, `task::await`, `task::yield_now` implemented as a state-machine generator in codegen.
- [ ] Poller backed by `mio`, single OS thread initially.
- [ ] `net::tcp` async variants (`connect_async`, `recv_async`) return handles that the poller wakes.
- [ ] Timers via `setTimeout`/`setInterval` in `builtin/` TS layer calling into `task::schedule_after_ms`.

---

## Phase F — Build & distribution

### F1. Remove the two-step build
- [ ] Today: first `cargo build --release` writes an empty placeholder, second captures the real staticlib.
- [ ] Add a `cargo xtask build-release` helper that invokes `cargo build --release --lib` followed by `cargo build --release --bin rts`.
- [ ] Or: use a build-dependency crate that produces the staticlib explicitly before the main crate compiles.

### F2. Option B — per-namespace slicing
- [ ] Split each namespace into its own small staticlib (workspace members `rts-io`, `rts-fs`, `rts-crypto`, ...).
- [ ] `build.rs` embeds each `.lib` under a symbol name.
- [ ] At link time, only include staticlibs whose namespaces are actually referenced by the user program.
- [ ] Expected binary size for "hello world": <200 KB vs today's 510 KB.

### F3. `rts i` installer
- [ ] `build.rs` produces `builtin.tar` from `builtin/` plus `manifest.json` with per-file sha256.
- [ ] `include_bytes!` the tarball into the binary.
- [ ] `rts i` subcommand extracts under `<project>/builtin/`, writing only files whose sha diverges.
- [ ] Resolver prefers project-local `builtin/` over the embedded copy so users can patch prototypes.

### F4. Cross-platform link auto-detection
- [ ] Current MSVC discovery works on Windows.
- [ ] Add ld/gcc lookup for Linux, clang/ld for macOS.
- [ ] Respect `RTS_LINKER=<path>` env var as an override for CI.

---

## Phase G — Source maps & diagnostics

- [ ] Emit DWARF during codegen (Cranelift supports it via `cranelift_codegen::isa::TargetIsa::emit_unwind_info`).
- [ ] Map user `.ts` source lines to binary PCs.
- [ ] Runtime panic handler reads the DWARF from the binary to print a file:line backtrace.
- [ ] Replace the `ignored.reason: String` warnings with structured diagnostics routed through `DiagnosticEngine`.

---

## Phase H — LSP / DX

- [ ] TypeScript language server using the RTS type checker (not `tsc`).
- [ ] Autocomplete emits actual `__RTS_*` symbols so users can see what will be called.
- [ ] Inline hints showing which calls are "direct ABI" vs "runtime helper".
- [ ] `rts fmt` / `rts check` / `rts test` subcommands.

---

## Phase I — `builtin/` TypeScript prototype layer

Bring back the idea of JS value classes implemented in TS on top of raw ABI:

- [ ] `builtin/rts/types.ts` — ambient declarations for every `__RTS_FN_NS_*` symbol (auto-generated from `abi::SPECS`).
- [ ] `builtin/string.ts` — `class String { _handle: number; get length(); slice(...); concat(...); }`.
- [ ] `builtin/number.ts`, `boolean.ts`, `array.ts`, `object.ts`, `date.ts`, `math.ts`, `console.ts`, `error.ts`.
- [ ] `builtin/node/*` — Node.js compatibility shims (`fs`, `path`, `process`, `net`).
- [ ] Users extend prototypes freely; codegen specialises known types to raw handles.

Depends on Phase A (string handles, concat, method calls) and Phase B (type checker can see builtin classes).

---

## Phase J — Semver 1.0

- [ ] Freeze `abi::SPECS` surface.
- [ ] CI gates: symbol uniqueness, ABI stability (byte-compare disassembly of `__RTS_*` between builds), `rts.d.ts` in sync with SPECS (already enforced by `types-lint.yml`).
- [ ] Publish pre-built `rts-<target>.zip` per platform.
- [ ] Docs site generated from SPEC docstrings.

---

## Done

- [x] Legacy dispatch / `RuntimeValue` / MIR / HIR excised.
- [x] New ABI layer: `src/abi/` com `NamespaceSpec`, `NamespaceMember`, `AbiType`, guards, signatures, symbols.
- [x] `src/abi/mod.rs` dividido em submódulos (`guards`, `member`, `signature`, `symbols`, `types`).
- [x] Namespaces `gc`, `io`, `fs` implementados na nova ABI com `SPEC` + `__RTS_FN_NS_*` symbols.
- [x] Codegen bootstrap: `src/codegen/emit.rs` emite `main` com chamadas literais a namespaces registrados.
- [x] Extrator `src/codegen/program.rs`: reconhece `<ns>.<fn>(literal...)`, ignora o resto com aviso estruturado.
- [x] Pipeline `src/pipeline.rs`: parse → codegen → link → executa.
- [x] `rts run` e `rts compile` funcionando no novo pipeline.
- [x] Runtime embutido (`src/runtime/embedded.rs`): staticlib RTS auto-extraída e linkada.
- [x] MSVC toolchain discovery para Windows (walks `ProgramFiles`/`ProgramFiles(x86)`).
- [x] `rts.d.ts` gerado automaticamente por `render_typescript_declarations()`.
- [x] CI `types-lint.yml`: verifica `rts.d.ts` commitada contra saída do gerador.
- [x] `builtin/` renomeado de `packages/` — `console`, `globals`, `rts-types` realocados.
- [x] ABI `__rts_crypto_sha256_direct` com zero overhead + microbench de dispatch.
- [x] i32 specialization no codegen + fix de regressão de performance em loops.
- [x] `gc-arena` adicionado ao `Cargo.toml` (pronto para integração na Phase D).

---

## Known debt / cleanup

- `src/runtime/embedded.rs` — empty-payload error message currently mentions `cargo build` twice; replace with an automatic second-pass build (F1).
- `src/linker/system_linker.rs` — MSVC discovery walks common install roots; add vswhere-backed lookup for non-default installs.
- `src/cli/mod.rs` — `fnv1a32` + `render_error` helpers kept as `#[allow(dead_code)]`; wire back in once diagnostics routing stabilises.
- `src/cli/init.rs` — `emit_rts_dts` is a minimal stub; regenerate split per-namespace `.d.ts` files once the generator from legacy is ported.
