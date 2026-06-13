# Runtime — HandleTable, tokio, GC, State

## Shard-aware HandleTable

`HandleTable` is split into 32 mutually lock-free shards. `alloc_entry`
distributes round-robin by thread; `shard_for_handle` decodes O(1) the shard of
any handle (encoded in the low bits). All 17+ handle-based namespaces migrated to
this API — no contention in parallel workloads.

## Shared tokio runtime (issue #399)

`crates/rts-runtime/src/runtime/async_rt.rs` exports `rt()` — a global
multi-thread `OnceLock<tokio::runtime::Runtime>`. The `on_thread_start`/
`on_thread_stop` hooks register each worker in `gc/thread_registry` so the GC
scanner sees live handles in tokio tasks (without this the sweep collected them
wrongly under concurrent load).

Every async feature must reuse this runtime instead of creating its own:

- `http_server::serve` calls `rt().block_on(...)`
- `thread::spawn_async*` uses `rt().handle().spawn_blocking(...)`
- `runtime::tokio_ctx` offers "opaque u64 id + shard map by TypeId" as a generic
  sync↔async bridge (replaces http_server's ad-hoc `slots()`)
- `promise.create` (drysius design, #437) calls `rt.spawn_blocking(...)` to
  invoke the fn handle and settle the Promise

Convention: what crosses the JIT (extern "C") is only an opaque u64. Rust-rich
types (Arc<T>, Channel, JoinHandle, JITModule) live in the shard map keyed by
that id — or in GC handles with a lifetime guard
(`Entry::Function::keep_alive`).

## GC stack scanner Win32

`mark_stack_roots()` in `crates/rts-runtime/src/namespaces/gc/collector.rs` uses
`GetCurrentThreadStackLimits` (official Win32 API) instead of the TIB's
`gs:[0x10]`. In some contexts TIB.StackBase returned a value < RSP, leaving the
scanner marking nothing and the sweep collecting live handles (bug found
2026-05-01 testing http_server under load). The same path scans threads in the
`thread_registry` via `SuspendThread + GetThreadContext` + callee-saved register
scan.

## GC — mark+sweep with Cranelift stack maps

**Current state:** precise mark+sweep using
Cranelift's `UserStackMap`, with a conservative scanner via `SuspendThread +
GetThreadContext` to cover all RTS threads registered in `thread_registry`.
Details:

- Codegen calls `builder.declare_value_needs_stack_map(val)` for each handle
- `jit.rs` extracts `UserStackMap` after `define_function` and registers absolute
  return-PCs in `stack_map_registry`
- Every N allocations (`GC_TICK_INTERVAL = 256`), `finish_cycle()` runs
  `mark_stack_roots()` (scans the current thread's stack + other threads' stacks
  via SuspendThread) and `sweep_all_shards()` frees what was not marked
- `mark_stack_roots()` on Windows uses `GetCurrentThreadStackLimits` (official
  Win32 API). Do not use `gs:[0x10]` — in some contexts it returns StackBase <
  RSP, leaving the scanner marking nothing and the sweep collecting live handles
  (bug PR #400)

### Required change for the new engine — recognize NaN-boxed PolyValue roots

The conservative stack scanner today scans words looking for `u64` handles. The
redesign's `PolyValue` (NaN-box) value model requires the scanner to also
**recognize a boxed-handle word** and extract its slot (design doc §5.4, Pilar 1):

- A stack word `w` is a potential root iff `(w & BOX_BASE) == BOX_BASE` (with
  `BOX_BASE = 0xFFF8_0000_0000_0000`) **and** `tag(w) ∈ {STR, OBJECT, FUNCTION}`;
  the root is the 48-bit `slot(w)` (slot+shard), which is a HandleTable slot
  index — never a raw pointer.
- Inline ints, inline floats, and singletons are **not** roots (they reference no
  heap). This is *more* precise than today: float words that merely look like
  handles stop being false positives.
- The 16-bit handle generation does not fit in the 48-bit payload; generation is
  validated slab-side, and a live PolyValue keeps its slot reachable (so a stale
  read cannot happen for live values). Only WeakRef/FinalizationRegistry need the
  full 64-bit `(slot, generation)` handle (design doc §5.5, ties to #217).

## Runtime vs Compile

Two execution paths sharing the same Cranelift codegen:

- **`rts run`**: compiles directly to executable memory via `JITModule`. No disk,
  no external linker. All ABI symbols are registered in `JITBuilder::symbol` at
  JIT module startup (the JIT emitter; in the new engine the symbol table is
  derived from `SPECS` via `abi_gen.rs`, not hand-written `add_fn!`).
- **`rts compile`**: applies use-slicing, generates only the objects of the
  effectively used modules, produces the final binary.

`FnCtx.module` is `&mut dyn Module` — `ObjectModule` and `JITModule` implement
the same trait and pass through the same `compile_program` pipeline.

Object naming convention: `<module>.o` (and `.m` when there is metadata for the
incremental cache).

## State

Namespace state uses `Arc<Mutex<T>>` directly when needed, or `thread_local!`
for per-thread caches. There is no central state system — each namespace manages
its own.

### Pattern for shared state

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

static FS_STATE: std::sync::OnceLock<Arc<Mutex<FsState>>> = std::sync::OnceLock::new();

fn fs_state() -> Arc<Mutex<FsState>> {
    FS_STATE.get_or_init(|| Arc::new(Mutex::new(FsState::default()))).clone()
}

#[derive(Default)]
struct FsState {
    open_files: HashMap<u64, std::fs::File>,
}
```

### Pattern for thread-local caches

```rust
use std::cell::RefCell;

thread_local! {
    static EXPR_CACHE: RefCell<HashMap<u64, Expression>> = RefCell::new(HashMap::new());
}

pub fn reset_cache() {
    EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
}
```

## No legacy code

**Absolute rule: dead code is removed immediately. Never comment out, never
leave "just in case".**

- Any code not reached by any live path must be deleted in the same commit that
  killed it
- `todo!()` / `unimplemented!()` stubs are acceptable as temporary WIP markers;
  commented code is not
- `dead_code` warnings are treated as errors — the build cannot finish with
  warnings
