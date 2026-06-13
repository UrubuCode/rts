# Architecture — project, ABI, namespaces

## Project

RTS is a TypeScript-to-native compiler/runtime using Cranelift as codegen
backend. Goal: compile TS/JS to native binaries with a minimal Rust runtime,
shipped as a standalone toolchain (no external runtime support library).

The runtime layer is organized around the `rts-engine::abi` + `SPECS` contract,
with a module-graph pipeline + incremental cache. Two execution paths: JIT via
`cranelift_jit::JITModule` (direct executable memory, `rts run`) and AOT via
`cranelift_object::ObjectModule` (external linker, `rts compile`).

The canonical engine direction is `docs/specs/rts-codegen-new-design.md`.

## Architecture

Cargo workspace in `crates/`. `src/` is the facade of the `rts` bin
(re-exports); real paths live under `crates/<crate>/src/`.

> **PRIMORDIAL-vs-Registry doctrine + crate partition** (see `CLAUDE.md` §
> "MANDATORY RULE: PRIMORDIAL-vs-REGISTRY DOCTRINE", which **survives the
> redesign**). The engine may name ONLY the primordial classes
> (String/Object/Array/Function/Promise/Boolean/Number/Error+subclasses);
> everything else resolves via the Registry (`global_class_lookup`,
> `instanceof_predicate`, member metadata), zero hardcoded mention, no builtins
> in the engine. The runtime layer is partitioned: `rts-engine` ←
> `rts-primitives` (primordials) + `rts-shared` (non-primordial universal) ←
> `rts-std` ← `rts-runtime` facade. The ABI contract lives in `rts-engine::abi`.

> **Two codegen crates during the strangler-fig migration.**
> `crates/rts-codegen-old/` is the **frozen** old engine (dual HIR→MIR /
> authoritative-AST path, overloaded-`i64` value model, 4.6k-LOC switchboard,
> 1113 manual `add_fn!`) — still plugged into the bin/cli until cutover.
> `crates/rts-codegen-new/` is the **active redesign** (single HIR→Cranelift
> lowering, `PolyValue` NaN-box, shapes + data ICs, data-driven dispatch +
> generated ABI). Canonical design: `docs/specs/rts-codegen-new-design.md`.

```
crates/
  rts-ast/          — internal AST
  rts-parser/       — SWC parse + AST; arrow/fn expressions → top-level Item::Function
  rts-diagnostics/  — structured errors
  rts-engine/       — heap GC + ABI contract (abi:: SPECS, types, symbols,
                      signatures, Intrinsic, global_class, handles) + Registry/builder
  rts-hir/          — typed HIR (HirType I8..I128/F32/F64/Bool/Str/Handle/Array/Function/Class/Object/Any/Unknown)
  rts-mir/          — SSA MIR — used ONLY by rts-codegen-old (frozen); the redesign deletes the MIR tier
  rts-codegen-old/  — FROZEN old engine (dual MIR/AST codegen, switchboard, manual add_fn!)
  rts-codegen-new/  — ACTIVE redesign (see module map below)
    src/value.rs    — PolyValue (64-bit NaN-box) — Pilar 1
    src/repr.rs     — Repr lattice (Int32/Float64/Bool/Ref/Tagged) + join — Pilar 2 (soundness core)
    src/shape.rs    — hidden classes (Shape / transition tree / slot layout) — Pilar 4
    src/ic.rs       — AOT-safe data inline caches (PropIcCell, uninit→mono→poly→mega) — Pilar 4
    src/dispatch.rs — data-driven method resolution (Target / resolve_method) — Pilar 6
    src/abi_gen.rs  — JIT symbol table derived from SPECS (SymbolEntry / jit_symbols) — Pilar 6
    src/lower/      — single HIR → Cranelift lowering path (no MIR) — Pilar 5
    src/pipeline.rs — shared JIT (run_jit) + AOT (compile_aot) — Pilar 5
  rts-primitives/   — PRIMORDIAL classes
  rts-shared/       — non-primordial universal (math/num/collections/json/globals)
  rts-std/          — backend (io/net/tokio/console/promise impl)
  rts-runtime/      — facade ("rts" + "rts:<ns>" submodules); AOT staticlib
  rts-node/         — Node.js builtin shims (fs, os, path, process, crypto, util)
  rts-linker/       — native link (system linker with object-backend fallback)
  rts-cli/          — CLI (run, compile, apis, init, repl, eval, ir)

src/                — bin facade (re-exports), runtime_objects.rs, main.rs
```

### New-engine pipeline (the redesign — single path, no MIR)

```
TS → SWC → AST → HIR (rts-hir) → lower/ (HIR → Cranelift IR, one path) → Cranelift egraph → JIT/AOT
```

There is **no MIR tier and no dual AST/MIR codegen** in `rts-codegen-new`. The
Cranelift egraph (`use_egraphs=true`) is the **sole** optimizer. The front-end
does only what Cranelift cannot (JS semantics): coercions, the polymorphic `+`,
box/unbox insertion (pure IR the egraph folds), shape/IC site emission,
narrow-int wrap, exception edges. See `docs/specs/rts-codegen-new-design.md` §9.

`FnCtx.module` is `&mut dyn Module` to serve AOT and JIT without duplicating
codegen (`compile_program` shared, `pipeline.rs`).

> The frozen `rts-codegen-old/` still runs the old hybrid HIR→MIR→Cranelift
> (default, gated by `RTS_USE_MIR`) with authoritative-AST fallback. That
> machinery is NOT carried into the new engine.

## ABI (`rts-engine::abi`) — single contract

All surface between codegen and runtime goes through `rts-engine::abi`. No more
per-namespace `SPEC/MEMBERS/dispatch()`, no more `__rts_call_dispatch`.

- `abi::SPECS` (`mod.rs`) — static slice with the `NamespaceSpec` of each
  registered namespace (`io`, `fs`, `gc`, `math`, `bigfloat`, …). Single source
  consumed by codegen, runtime, JIT, and the `rts.d.ts` generator. In the new
  engine, `abi_gen.rs` **derives** the JIT symbol table from these SPECS (no
  manual `add_fn!`), with a build-time coverage assertion.
- `abi::lookup(qualified)` — resolves `"io.print"` → `&NamespaceMember` with
  symbol and signature.
- `member.rs` — `NamespaceSpec`, `NamespaceMember` (static consts) and
  `Intrinsic` (enum of inlinable ops). Each member declares `name`, `kind`
  (Function|Constant), `symbol`, `args[]`, `returns`, `doc`, `ts_signature`,
  `intrinsic: Option<Intrinsic>`. When `intrinsic` is `Some`, codegen emits
  Cranelift IR directly instead of `call <symbol>`.
- `types.rs` — `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 | StrPtr |
  Handle`. `StrPtr` expands to two Cranelift slots (`ptr` + `len`).
- `signature.rs` — `lower_member()` converts the spec to a Cranelift
  `LoweredSignature`.
- `symbols.rs` — convention `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` (e.g.
  `__RTS_FN_NS_IO_PRINT`). Macro `rts_sym!` generates symbols at compile time;
  `validate_symbol()` enforces uppercase ASCII.
- `guards.rs` — `guard_for(expected, caller)`. In the old engine this is **dead
  code** (zero production call sites; coercion is ad-hoc `TPL_COERCE_AUTO`). The
  redesign makes coercion ONE real authority (design doc §7) — `guard_for` is
  promoted to the real path or replaced in `rts-codegen-new`.

The monomorphic numeric path still emits `call <symbol>` directly via Cranelift,
no intermediaries; PolyValues cross the runtime boundary tagged-in/tagged-out for
the generic calls (design doc §10.3).

## Machine ABI — typed extern "C", no dispatch

No `JsValue`, no `__rts_call_dispatch`, no boxing at the codegen/runtime
boundary. Each namespace function is a typed `extern "C"` symbol.

### ABI convention by type

| TS type  | `AbiType`    | Cranelift repr                  | Note                                            |
|----------|--------------|---------------------------------|-------------------------------------------------|
| `number` | `I64` / `F64`| `i64` / `f64`                   | native bits, no boxing                          |
| `bool`   | `Bool`       | `i8` (0/1)                      | 0 = false, 1 = true                             |
| `string` | `StrPtr`     | 2 slots: `(i64 ptr, i64 len)`   | UTF-8; static codegen ptr, or GC-handle buffer  |
| handle   | `Handle`     | `u64`                           | `HandleTable` (gen:16 + slot:48)                |
| void     | `Void`       | —                               | no return                                       |
| ints     | `I32` / `U64`| `i32` / `u64`                   | counts, status, sizes                           |

### Implementation rules

- Each namespace member becomes `#[unsafe(no_mangle)] pub extern "C" fn
  __RTS_FN_NS_<NS>_<NAME>(...)`
- No namespace fn accepts/returns `JsValue` at the `extern "C"` boundary
- Dynamic strings (e.g. read results) are allocated by `gc` and return a `u64`
  handle; read via `gc::string_ptr(handle)` + `gc::string_len(handle)`
- In the new engine, coercion/`any` handling goes through the single coercion
  authority (design doc §7, Pilar 3), not scattered ad-hoc `TPL_COERCE_AUTO`

## Per-namespace file structure

```
crates/rts-runtime/src/namespaces/<ns>/
  mod.rs         — re-exports submodules and publishes the NamespaceSpec
  abi.rs         — NamespaceMember declarations (static table)
  <group>.rs     — operational impl (e.g. read.rs, write.rs, dir.rs, print.rs, stdout.rs, ...)
```

Rules:
- `mod.rs` is only the import map + `NamespaceSpec` export
- `abi.rs` is the source of truth for namespace members (name, symbol, args,
  return, doc, ts)
- Each operational file groups functions by responsibility
  (io/r-w/dir/metadata/...)
- No per-namespace `dispatch()` — each function is a direct `#[no_mangle] extern
  "C"`

Active namespaces (40+): `io`, `fs`, `gc`, `math`, `num`, `bigfloat`, `time`,
`env`, `path`, `buffer`, `string`, `process`, `os`, `collections`, `hash`,
`fmt`, `crypto`, `net`, `tls`, `thread`, `atomic`, `sync`, `parallel`, `mem`,
`hint`, `ptr`, `ffi`, `regex`, `runtime`, `test`, `trace`, `ui`, `alloc`,
`json`, `date`, `http_server`, `promise`, `events`, plus the `globals/`
sub-namespaces (number, string, date, regexp, error, events, console, json,
timers, fetch, performance, global_this, text_encoding, url). Covers std::* +
parallelism + HTTPS + UI + JSON + Date + native HTTP server (actix-web) + full
global JS classes.

### Existing namespaces

- `io/` — print, eprint, stdout_{write,flush}, stderr_{write,flush},
  stdin_{read,read_line}
- `fs/` — read, read_all, write, append, exists, is_file, is_dir, size,
  modified_ms, create_dir(_all), remove_dir(_all), remove_file, rename, copy
- `gc/` — handles and string pool: string_from_{i64,f64,static},
  string_{new,concat,len,ptr,free}, slab-based `HandleTable` with 16-bit
  generation + 48-bit slot (`u64` handle); `Entry` enumerates stored types
  (`String`, `BigFixed`, `Buffer`, `ProcessChild`, `Map`, `Vec`, `Function`,
  `PromiseAsync`, `Free`)
- `math/` — basic
  (floor/ceil/round/trunc/sqrt/cbrt/pow/exp/ln/log2/log10/abs_f64/abs_i64),
  trig (sin/cos/tan/asin/acos/atan/atan2), minmax (min/max/clamp_f64/i64),
  consts (PI/E/INFINITY/NAN as `MemberKind::Constant`), random (xorshift64 with
  state in `__RTS_DATA_NS_MATH_RNG_STATE`). Intrinsics:
  sqrt/abs_f64/min_f64/max_f64/abs_i64/min_i64/max_i64/random_f64
- `bigfloat/` — i128 decimal fixed-point (decimal scale up to 36). Operations:
  zero/from_f64/from_i64/from_str/to_f64/to_string/add/sub/mul/div/neg/sqrt/free.
  Used for pi with 29+ digits via Machin + Maclaurin atan
- `time/` — now_ms/now_ns (monotonic Instant), unix_ms/unix_ns (SystemTime),
  sleep_ms/sleep_ns
- `env/` — get_var, set_var, remove_var, args_count, arg_at, cwd, set_cwd
- `path/` — join, parent, file_name, stem, ext, is_absolute, normalize,
  with_ext (pure operations, no I/O)
- `buffer/` — Vec<u8> via HandleTable: alloc/alloc_zeroed/free/len/ptr,
  read/write u8/i32/i64/f64 little-endian, copy/fill, to_string (UTF-8)
- `string/` — search (contains/starts_with/ends_with/find), transform
  (to_upper/to_lower/trim/trim_start/trim_end/repeat), replace/replacen,
  char_count/byte_len/char_at/char_code_at (Unicode-aware)
- `process/` — exit/abort, pid, args_count/arg_at (alias of env), spawn (args
  separated by \n), wait (consumes handle), kill. Child handle via
  `Entry::ProcessChild`
- `os/` — platform/arch/family/eol (std::env::consts + cfg!), home_dir,
  temp_dir, config_dir, cache_dir (XDG on Unix, APPDATA/LOCALAPPDATA on Windows)
- `collections/` — HashMap<string, i64> (`map_*`) and Vec<i64> (`vec_*`) via
  HandleTable. Value always i64 — caller interprets as int/handle/bool
- `hash/` — deterministic SipHash-2-4 for str/i64/bytes (hash_str, hash_i64,
  hash_bytes)
- `fmt/` — parse_i64/f64 (tolerant), fmt_hex/oct/bin/f64_prec
- `crypto/` — inline SHA-256 (FIPS 180-4), base64/hex encode+decode, CSPRNG via
  BCryptGenRandom (Windows) / /dev/urandom (Unix)
- `net/` — TCP listener/stream + UDP socket + DNS resolve via `std::net`.
  Handles via `Entry::TcpListener/TcpStream/UdpSocket(UdpEntry)`. Sync, no
  external deps
- `tls/` — TLS 1.2/1.3 client via `rustls` + `webpki-roots` (embedded Mozilla
  CAs). Wraps `TcpStream` in a TLS connection. HTTPS works end-to-end without
  OpenSSL or schannel
- `thread/` — 4 coexisting mechanisms, dev chooses by workload: `spawn` +
  `join`/`detach` (`std::thread`, real JoinHandle, ~30k spawn/s, good for long
  CPU-bound); `spawn_async_join` + `join_async` (tokio `spawn_blocking`, returns
  i64, ~400k spawn/s, good for light/IO); `spawn_async` (tokio fire-and-forget,
  ~400k spawn/s); `spawn_detached` (fixed 8-worker pool, 5M spawn/s but unbounded
  queue — OOM risk). Plus `scope` auto-join + `sleep_ms`. Doc-comments in
  `crates/rts-runtime/src/namespaces/thread/abi.rs` have a comparison table
- `http_server/` — native HTTP/1.1 server via `actix-web` over the shared tokio
  runtime. Sync→async bridge: `serve(addr,handler)` blocks, each request enters
  a shard map of slots, the TS handler is called directly on the async thread,
  the response returns via oneshot. Supports keep-alive, pipelining, correct
  parsing. Measured peak 29k req/s (78% of pure-Rust actix)
- `atomic/` — `std::sync::atomic`: AtomicI64 (load/store/fetch_*/cas/swap),
  AtomicBool, AtomicF64 (via AtomicU64 + bit-transmute), fences
- `sync/` — `std::sync`: Mutex<i64>, RwLock<i64>, Once. Thread-local guards to
  cross extern "C" calls
- `parallel/` — `rayon`: map/for_each/reduce + num_threads. Backed the OLD
  engine's silent-parallelism passes (purity_pass, reduce_pass,
  array_methods_pass; frozen in `rts-codegen-old`, not carried into the new
  engine unless re-justified)
- `mem/` — size_of/align_of constants, swap_i64, drop/forget_handle
- `num/` — checked/saturating/wrapping arith, bit ops (rotate,
  count_ones/zeros, leading/trailing_zeros, reverse_bits, swap_bytes), bitcast
  f64<->bits
- `ptr/` — copy_nonoverlapping, raw pointer ops
- `ffi/` — CString, OsString
- `regex/` — `regex` crate backend, compile + test/find/replace/replace_all
- `runtime/` — eval_file (dynamic import) + eval (compiles TS source at runtime
  via `runtime_eval_src_jit`) + hot-reload primitives
- `test/` — test_core (suite/case begin/end, fail) + bundle.ts (`rts:test`
  describe/test/expect)
- `trace/` — push/pop/capture/print frame stack for Bun-style errors
- `ui/` — FLTK 1.x bindings (Button, Window, Input, Slider, ...)
- `alloc/` — malloc-style raw allocations
- `hint/` — black_box, spin_loop, unreachable, assert_unchecked
- `events/` — primitive EventEmitter: emitter_new/free, on, emit0/emit1,
  listener_count, remove_all_listeners
