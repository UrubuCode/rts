# Guide: how to add a namespace (module) or a class

> **Rewritten 2026-07-28.** The previous version described `src/abi/mod.rs`,
> hand-written `abi.rs` MEMBERS tables and a branch (`feat/remake-namespaces`)
> that no longer exists. It instructed exactly the pattern the SINGLE SOURCE OF
> TRUTH rule now forbids. Nothing of it survives.
>
> Binding context you must know before using this guide:
> - **`CLAUDE.md` § MANDATORY RULE: SINGLE SOURCE OF TRUTH** — `rts-macro`
>   declares, `rts-symbol-baker` bakes. Never hand-write a symbol row.
> - **`docs/specs/rts-macro-single-source.md`** — the spec, incl. phases F0→F13.
> - **`CLAUDE.md` § PRIMORDIAL-vs-REGISTRY DOCTRINE** — the engine names only
>   primordials; your namespace is reached by DATA, never by a codegen arm.

## 0. The one-paragraph model

A namespace (a `rts:<ns>` / `node:<mod>` module) or a class is **metadata pushed
into the Registry** by a `pub fn register(e: &mut Engine)`, plus the real
`extern "C"` functions that metadata points at. The engine never learns your
name: it resolves `import { x } from "rts:foo"` and `recv.method(a)` through one
generic path over that data. Your job is to declare the surface and add one row
to one table.

## 1. Where the code goes

Pick the crate by what the thing IS. This is the layering, and it is a SEMANTIC
question — not a permission question: the dependency-direction bans were removed
2026-07-28.

| Kind | Crate |
|---|---|
| PRIMORDIAL class (String/Object/Array/Function/Promise/Boolean/Number/Error/Symbol/BigInt/Proxy/Reflect/TypedArrays/Math…) | `rts-primitives` |
| Non-primordial UNIVERSAL surface (math/num/collections/json/…) | `rts-shared` |
| BACKEND (io/net/tokio/console/fs/…) | `rts-std` |
| `node:<mod>` compatibility | `rts-node` |

```
crates/<crate>/src/<ns>/
  mod.rs        — the `pub fn register(e: &mut Engine)` + re-exports
  <group>.rs    — the implementation, grouped by responsibility (read/write/dir/…)
```

`mod.rs` is the declaration; the operational files hold the logic. Keep every
file under its ceiling (rest ≤500 lines — split into a subfolder, never append
to an oversized file).

## 2. Declaring the surface

Two live authoring forms. **Prefer the macro** — it is source of truth #1, and
its `SymbolDesc` is derived from the Rust signature, so the symbol name and the
ABI signature cannot drift from the function they describe.

### 2a. `#[rtse::*]` — the target form

A class is a normal Rust struct + `impl`; the macro adds the ABI without changing
the struct's ordinary Rust usability (the PyO3/napi-rs model):

```rust
struct Point { x: f64, y: f64 }

#[rtse::class("Point")]
impl Point {
    #[rtse::ctor]     fn new(x: f64, y: f64) -> Self { Point { x, y } }
    #[rtse::method]   fn sum(&self) -> f64 { self.x + self.y }
    #[rtse::getter]   fn label(&self) -> String { format!("({},{})", self.x, self.y) }
    #[rtse::statical] fn origin() -> Self { Point { x: 0.0, y: 0.0 } }
}
```

Emits, per member: the `extern "C"` wrapper (marshalling handle↔struct and
Rust↔ABI), its `SymbolDesc` const, and a `pub fn register(e: &mut Engine)`.

Available: `ctor`, `method` (`&self` / `&mut self`), `statical` (the marker is
`statical`, not the `static` keyword), `variable` (scalar field → getter/setter
pair), `getter`/`setter`, `private`, `asynch` (a real `async fn`, driven by
`rts_engine::block_on`, returned as a settled Promise), `constant`,
`symbol(...)` (key a member by `Symbol.for("x")` or the well-known
`Symbol::iterator`), `optional=N` (last N params default `undefined`), `throws`.
Overload by ARITY is free — N members with the same JS `name=` and distinct argc;
the engine dispatches on argc.

Params: `f64`, `i64`, `i32`, `bool`, `&str`, `Handle`/`u64`. Returns:
`String`/`&str`/`()`, `Option<Self>` (fallible ctor → JS `null`),
`Option<String>` (→ `string | null`), `Vec<String>`/`Vec<Handle>` (→ a real
array).

A free function with no class uses `#[rtse::function("name")]`; a bare
`extern "C"` the engine calls directly uses `#[rtse::abi(...)]`.

> **Instance methods CLONE the receiver** out of the HandleTable before running
> the body (dropping the shard lock; writing back for `&mut self`), so a body
> touching a second handle cannot self-deadlock. Your struct must be `Clone`.

### 2b. The builder — the form most of the tree still uses

```rust
pub fn register(e: &mut Engine) {
    e.module("serde", |m| {
        m.doc("Deep binary serialization (pickle): any value graph.");
        m.registry(throws(serialize_entry()));
        m.registry(throws(deserialize_entry()));
    });
}
```

The closure-scoped form (`e.module(name, |m| …)`) commits ONCE, at the end, so a
partial module cannot be registered. The older fluent form (`e.ns("alloc")
.member(func(...))`) is equivalent and still everywhere; both are fine to extend.
But note that writing a member by hand here means hand-typing a symbol name and
a `Sig` — the drift the macro exists to kill. Convert to `#[rtse::*]` whenever
the member's shape allows it.

## 3. The symbol name is DERIVED — never invent one

The convention (`rts_abi::scope` — ONE implementation, shared by the macro and
the baker, which is why it lives in dependency-free `rts-abi` and not in the
proc-macro):

```text
__rtsm_<module>_<value>          module symbol   __rtsm_io_print, __rtsm_node_fs_readFile
__rtsm_global_<class>_<value>    global class    __rtsm_global_string_toUpperCase
__rtsm_global_<value>            bare global     __rtsm_global_parseInt
__rtsn_<value>                   NATIVE          Rust helpers covering a Cranelift gap
                                                 (the `n` is NATIVE, NOT "node")
__rtsa_<value>                   ABI             the codegen↔runtime contract
```

Separator is always `_`, never `-`; a module specifier is normalized
(`node:fs` → `node_fs`, `Intl.NumberFormat` → `intl_numberformat`). The trailing
segment is the JS spelling **verbatim** — case preserved, because folding it
collides members that differ only by case.

`#[rtse::abi]` with no args gives the legacy `__`-prefixed verbatim form (a
transitional spelling being drained). Declare the scope instead:

```rust
#[rtse::abi(module = "node:fs", value = "readFileSync")]
pub fn node_fs_read_file_sync(path: &str) -> String { … }
```

The older `__RTS_FN_NS_<NS>_<NAME>` / `__RTS_FN_GL_<CLASS>_<NAME>` names are the
previous convention. They exist in bulk and still work — do not mass-rename, but
never spell a NEW one by hand.

## 4. Register it — one row

Add your `register` fn to `REGISTER` in
`crates/rts-codegen-new/src/front/run/registry_build.rs`:

```rust
pub(super) static REGISTER: &[fn(&mut Engine)] = &[
    // …
    ns::myns::register,
];
```

Order does not matter (each row just pushes metadata). The identity lives in the
register fn itself — it names its own module (`e.ns("bigfloat")`,
`e.module("node:fs")`, `e.class("Date")`) — so the table carries no label strings
to keep in sync.

A `.ts` prelude goes in `PRELUDE_TS` in the same file, where **order DOES
matter**: the merged prelude is one program, so a base must precede its
dependents (`ERROR_TS` before the `extends Error` subclasses; primordials before
`rts:test`'s bundle).

## 5. Re-bake the symbol table

After adding or renaming any symbol:

```bash
cargo run -p rts-symbol-baker            # regenerate; commit the artefact
cargo run -p rts-symbol-baker -- --check # what the gate and CI run
```

The baker scans the declarations and emits
`crates/rts-symbol-baker/generated/symbol_table.rs` — static, in strictly
ascending symbol order — which is the JIT vtable **and** the AOT symbol set. A
symbol is in the table because it exists in the source. Forgetting to regenerate
is a HARD gate failure, not a silent runtime crash.

## 6. What you must NOT do

- **Do not add a row to a hand-written symbol/signature table.**
  `rts-codegen-new/src/adapter_symbols/list_a.rs` + `list_b.rs`,
  `rts-codegen-new/src/value/abi_sig.rs` and
  `rts-runtime/src/adapters/dispatch.rs` are DRAINING TARGETS (527 rows). The
  gate lists them; the list may only shrink.
- **Do not name your namespace or class in the engine.** No `if name == "myns"`,
  no `match`, no `const NAMES: &[&str]` allow-list in
  `crates/rts-codegen-new/`. If the lowering seems to need one, a SPEC is missing
  metadata — add the flag/field on the member and the generic path picks it up.
  (CLAUDE.md § ANTI-HARDCODE lists the four resolution strategies.)
- **Do not implement a high-level JS API in Rust.** Rust exposes raw primitives;
  the ergonomic surface is a `.ts` prelude over them.
- **Do not hand-write a `#[unsafe(no_mangle)] pub extern "C" fn` name.** Declare
  it and let the derivation produce it.

## 7. Checklist

1. `crates/<crate>/src/<ns>/mod.rs` + implementation files, each under its ceiling.
2. Surface declared with `#[rtse::*]` (preferred) or the `e.module(…)` builder.
3. `register` added to `REGISTER` (and any `.ts` to `PRELUDE_TS`, in dependency order).
4. `cargo run -p rts-symbol-baker` — artefact regenerated and committed.
5. `cargo check -p <crate>` while iterating (never `--release`; see the ITERATION
   SPEED rule).
6. `tests/<ns>.test.ts` covering non-happy paths too (empty, nested, in a loop,
   in try/catch, combined with an adjacent feature).
7. `bash scripts/read_before_commit.sh` — read the whole output.

## Related

- `docs/specs/rts-macro-single-source.md` — the source-of-truth spec (F0→F13).
- `docs/specs/rts-std-surface.md` — the approved public-surface redesign; read it
  before changing what a namespace EXPORTS.
- `docs/specs/rts-engine-dispatch.md` — the `rts-engine` builder/Registry design
  (registration half; the dispatch half is superseded).
- `docs/specs/rts-codegen-new-design.md` §10 — how the engine resolves this data.
