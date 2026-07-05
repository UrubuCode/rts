# RTS_ENGINE.md — Method dispatch engine (single resolution + single emission)

> **STATUS NOTE (2026-07-05).** Written for the OLD engine: the `builtins.rs`
> string-match this doc kills was DELETED with `rts-codegen-old` — the dispatch
> half is superseded by `rts-codegen-new-design.md` §10 (MethodSpec +
> `resolve_method`, ABI harvested from SPECS). What survives from here is the
> REGISTRATION half: the `rts-engine` builder/Registry design (§4, §9.5, §10
> external modules) that the live code references. Kept for those references
> and the design rationale; read the codegen design doc first.

> Status: **staged implementation in progress** (foundation started — see §0.1).
> Canonical document for the *dispatch* half of the engine (how `recv.method(args)`
> resolves to a native symbol without today's scattered string-match) plus, in §10,
> the **new mode** (generic engine + native external modules). Complements
> `docs/specs/rts-core-engine.md`, which covers the *registration* half.
>
> Non-negotiable principle: **100% native, zero interpretation.** Well-typed TS
> compiles to a direct `call <symbol>`, identical to today. Only a genuinely
> `any` receiver pays 1 runtime tag-check — a cost that already exists.
>
> **ID scheme:** `F*` = foundation, `E*` = dispatch (kills `builtins.rs`),
> `A*` = authoring/variables, `X*` = external modules (§10), `Q*` = quick-fixes of the
> critical items. The **single canonical roadmap + status** is in §0.1; the detailed
> per-section orderings (§6, §9.6, §10.8) are detail views pointing to it.

---

## 0. TL;DR

Today there are **two parallel method resolvers** doing the same thing:

1. **Table-driven** (`abi::global_class_lookup` → `GlobalClassSpec` →
   `lower_global_instance_call`) — clean, generic, drives Date/URL/RegExp/etc.
2. **String-match** (`builtins.rs`, ~182 `match method { "indexOf" => … }` arms)
   — hand-rolled, duplicates symbols and signatures the table already has.

For `String` both coexist: **34 spec rows AND ~50 string-match arms
pointing at the same `__RTS_FN_GL_STRING_*` symbols.**

The engine = **unify everything onto the table-driven path**, adding the two
missing pieces: (A) a **receiver-type** that survives to the call-site, and
(B) a **single entry gate** for resolution.

**But** the current foundation (`rts-macro` + registry + `lookup`) has **5
structural flaws** (§4) that, if the engine is built on top without fixing them,
become silent bugs multiplied. Correct order: **fix the foundation → then the
engine** (§6).

---

## 0.1 Status & unified roadmap (canonical)

This table is the **single source** of status. The step lists in §6/§9.6/§10.8
are detail. Working branch: `feat/engine-method-dispatch-1536` (issue #1536).

| ID | What | Section | Status |
|----|-------|-------|--------|
| **ENG0** | **`rts-engine`** crate (raw core): `Engine` + builder (module/class/global) + `Registry` (modules/classes/globals/jit_symbols, arity-keyed resolution) + `Member`/`FnPtr`/`Sig`/`sig!`. **Builder model** chosen; `rts-macro` will be removed (supersedes §9.1) | §10.2 | ✅ `fae05975` |
| **F1** | `NamespaceMember` += `aliases`/`variadic`/`default_args` + `DefaultArg` | §4.4 | ✅ `122e1392` |
| **A1a** | `MemberFlags` + `MemberKind` += `InstanceSetter`/`VarGetter`/`VarSetter` + `instance_setter` helper + exhaustive-match fallout | §9.2 | ✅ `1af5bea0` |
| **A2** | macro: `#[rts_module("scheme:name")]` · `#[rts_var(const\|let\|var,T,default)]` (atomic+GET/SET) · `#[rts_setter]` · `readonly`/`static_field` | §9.3/9.4 | ✅ `1af5bea0` |
| **F2a** | `GlobalClassSpec::resolve_instance_method(name,n_args)` arity-keyed (overload+alias+variadic+optional-tail) | §4.3 | ✅ `c2e1757f` |
| **F2b** | route the dispatch call-sites to `resolve_instance_method` (suite 1710/1710) | §4.3 | ✅ `fe894ab9` |
| **A3** | codegen: `VarGetter` read (`members.rs:1006`) + native write-path `x.v=5` + `readonly` hard-error + replace hardcoded `pathname`/`lastIndex` with `InstanceSetter` | §9.4/9.5 | ⬜ |
| **A4** | `GC_VAR_ROOTS` drain (JIT+AOT, `Handle` only) + var read/write/readonly-reject fixture | §9.4 | ⬜ |
| **Q1** | pin **Bool=i64** — fix the `ty.rs`/`types.rs` doc (it lies "i8"; `signature.rs:9` lowers `Bool→I64`) | §10.7 | ⬜ |
| **Q2** | move the symbol-switches `ns_call.rs:272`/`:314` to `MemberFlags` (`RAW_BITS_ARG`/`AMBIGUOUS_RET`) → data-driven emit | §10.7 | ⬜ |
| **F0** | **linkme** spike MSVC/COFF (Track B gate; rlib→bin) | §9.1 | ⬜ |
| **F3** | unified Registry Track A (`OnceLock<RwLock>`; `register_builtins()` drains the const arrays; `lookup`/`global_class_lookup` read the registry; route `for spec in GLOBAL_CLASS_SPECS`) | §10.2 | ⬜ |
| **F4** | jit `symbol_lookup_fn` (GetProcAddress/dlsym + `registry.jit_symbols`); shrink the 1104 `add_fn!` | §4.2 | ⬜ |
| **F5** | lint: no `__RTS_FN_*` literal outside a row; spec↔extern cross-check; null-Str = sentinel | §4.5/4.6 | ⬜ |
| **E0** | `engine/` — `RecvKind`, `MethodTarget` (incl. `UserClassMethod`/`VirtualDispatch`), `MethodIntrinsic`, `resolve_method`, `recv_kind` | §5 | ⬜ |
| **E1** | route user-class + global-class through the engine + divergence oracle | §5/§6 | ⬜ |
| **E2** | `Number` (~27 arms) → rows; delete `lower_number_builtin` | §6 | ⬜ |
| **E3** | `String` (~50 arms; aliases/default_args via macro) | §6 | ⬜ |
| **E4** | `Array`/`Map`/`Set` (`external` specs over `COLLECTIONS_*`; Tier-3 → `DISPATCH_AUTO`) | §5.5/§6 | ⬜ |
| **E5** | qualified-string ladder + bare-globals → registry; `#[rts_global]` (generic global scope, §9.7) + generic bare-ident resolution | §6/§9.7 | ⬜ |
| **E6** | (optional) `ValTy::Handle`→`Handle(HandleKind)`; class-id→vtable; engine shared with MIR | §6 | ⬜ |
| **X1** | dynamic-loading: `libloading` wrapper (LoadLibrary/dlopen/Mach-O) — **zero today** | §10.5 | ⬜ |
| **X2** | freeze `rts-abi::c_plugin` (repr(C) + `RTS_PLUGIN_ABI_VERSION` + fixed u8 codes) | §10.3 | ⬜ |
| **X3** | JIT loader (manifest+SHA256+`LoadLibrary`+`RtsHost`+register+intern) — **JIT-first milestone** | §10.5 | ⬜ |
| **X4** | `rts_plugin_entry!{}` + `cfg(rts_plugin)` arm in the macro + reference plugin (crc32) | §10.4 | ⬜ |
| **X5** | AOT externals: import-slot + `call_indirect` + `dlopen` init + `dylib` namespace + trap-stub — **NO-GO until proven** | §10.5 | ⬜ |

**Dependencies:** `X3` depends on `F3`+`F4`+`X1`+`X2`. The thesis "the registry is the
single gate / codegen without hardcode" (§10.7) only closes with `E2-E4` + GC root-set +
portable cross-thread scan. `Q1` is a prerequisite of any plugin extern with `Bool`.
`F2` (✅) is a prerequisite of overloads in `E*` and `X*`.

**Recommended next:** `F3` (inflection point — opens external modules and kills the
hand-written arrays) or the quick-fixes `Q1`/`Q2` (low risk, interleaveable).

---

## 1. Why an engine, and why it is NOT interpretation

RTS compiles TS → native via Cranelift. There is no JS engine. The risk the user
raised is legitimate: today the codegen "mentions the code directly" — it decides
which method to call by **string comparison of the name** (`"toString"`,
`"indexOf"`, …) spread over ~12 files. That is not runtime interpretation
(the final binary is native), but it is an **ad-hoc dispatch architecture**: the
"which method" decision is structurally encoded as a string-match in the
compiler, not as a data contract.

A real engine separates three responsibilities that are fused today:

| Responsibility | Today | Engine |
|---|---|---|
| **Declaring** a method (name, symbol, args, return) | `#[rts_method]` macro (partial) + literal in `builtins.rs` | `#[rts_method]` macro (single source) |
| **Resolving** `(receiver, method, arity)` → target | ordered string-match waterfall | `engine::resolve_method` (1 keyed lookup) |
| **Emitting** the target in IR | inline in each arm | `engine::emit_method_target` (1 emitter) |

The gain of "being an engine" is exactly that separation: declaring becomes data,
resolving becomes a lookup, emitting becomes one function. None of the three involves
interpreting TS at runtime. The numeric hot-path stays identical (the `rts ir`
invariant, §7).

---

## 2. Current state — what already works (and not to throw away)

The table-driven path **already proves the table can drive dispatch**:

```
abi::global_class_lookup("Date")            → &GlobalClassSpec
  .instance_method("getFullYear")           → &NamespaceMember
signature::lower_member(member)             → Cranelift signature
lower_global_instance_call(member, recv, …) → emits call <member.symbol>
```

`lower_global_instance_call` (`ns_call.rs:481`) already does **generic N-arg
marshalling**: `StrPtr`→ptr+len, `F64`→fcvt, `I32`→coerce, rest→i64, receiver in
slot 0. It drives constructors, static methods, getters, and instance methods
of ~60 global classes. The `#[rts_class]` macro generates the rows + the externs.

**The engine is an evolution of that path, not a new system.** Everything that follows
is "make 100% of the calls go through here".

---

## 3. Future (ideal) mode — concrete end-to-end example

### 3.1 Runtime: declare a primitive family as a class (once)

`Array`/`Map`/`Set` today **have no spec** — they are reached only via `builtins.rs`
calling `collections.vec_*`. In the future mode they become declared classes, with
rows pointing at the `__RTS_FN_NS_COLLECTIONS_VEC_*` symbols that **already exist**
(zero new runtime — `external` rows):

```rust
// crates/rts-core/src/namespaces/globals/array/mod.rs   (FUTURE)
/// Array.prototype — methods over a Vec<i64> handle.
#[rts_class(Array, prefix = "COLLECTIONS_VEC", spec = "ARRAY_CLASS_SPEC")]
impl Array {
    // Common case: single symbol, fixed arity → becomes a pure SymbolCall.
    #[rts_method(external, ts = "join(sep: string): string")]
    fn join(_recv: Handle, _sep: Handle) -> Handle { unreachable!() }

    // Arity overload: TWO rows, same `name`, distinct arities.
    // The resolver picks by arg count (impossible today — see §4.3).
    #[rts_method(external, ts = "indexOf(x: any): number")]
    fn index_of(_recv: Handle, _x: I64) -> I64 { unreachable!() }
    #[rts_method(external, name = "indexOf", symbol = "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM",
                 ts = "indexOf(x: any, from: number): number")]
    fn index_of_from(_recv: Handle, _x: I64, _from: I64) -> I64 { unreachable!() }

    // Variadic: NOT a fixed-arity row — marked `variadic`, emitted by a
    // residual handler that does the VEC_PUSH loop (see §5.3).
    #[rts_method(external, variadic, ts = "push(...items: any[]): number")]
    fn push(_recv: Handle, _items: I64) -> I64 { unreachable!() }

    // Default-arg: `end` optional; the EMITTER synthesizes i64::MIN when absent.
    #[rts_method(external, default_args = "[_, i64::MIN]", ts = "slice(start?: number, end?: number): any[]")]
    fn slice(_recv: Handle, _start: I64, _end: I64) -> Handle { unreachable!() }
}
```

`String`/`Number`/`Boolean` are already classes (`STRING_CLASS_SPEC` has 34 rows) —
they just gain `aliases`/`variadic`/`default_args` on the rows and lose the equivalent
`builtins.rs` arms.

### 3.2 Codegen: the whole call-site becomes three lines

```rust
// FUTURE — replaces the lower_string_builtin/lower_array_builtin/… waterfall
let recv = engine::recv_kind(ctx, &member.obj)?;          // RecvKind (1 point)
let target = engine::resolve_method(recv, &method, n_args) // keyed lookup
                 .ok_or_else(|| unknown_method(recv, &method))?;
return engine::emit_method_target(ctx, target, recv_val, call); // 1 emitter
```

No ordering, no "try string then array then map", no scattered `*_AUTO`.

### 3.3 What each call compiles to

```ts
const s: string = getName();
s.indexOf("x", 3);     // RecvKind::Class("String"), 2 args
```
→ resolves to the `indexOf/2` row → `SymbolCall(__RTS_FN_GL_STRING_INDEX_OF_FROM)`
→ **direct `call`**, identical to today's arm. Zero probing.

```ts
const a: number[] = [1,2,3];
a.indexOf(2, 1);       // RecvKind::Class("Array"), 2 args
```
→ `indexOf/2` row of `ARRAY_CLASS_SPEC` → `call __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM`.
**The same `(family, name, arity)` key disambiguates String vs Array** — what
today's try-order fakes.

```ts
arr.map(x => x.slice(1));   // x is an arrow param → type erased
```
→ `recv_kind(x)` = `RecvKind::Unknown` → `resolve_method` returns
`RuntimeAuto` → emits **one** `__RTS_FN_RT_DISPATCH_AUTO(handle, method_id, argv)`
that reads the `Entry` tag at runtime (`Entry::String`/`Vec`/`Map`) and branches.
**This is 1 tag-check + a native call, not interpretation.** It is exactly what today's 6
`*_AUTO` already do, generalized into one.

### 3.4 Observable invariant

`target/release/rts.exe ir bench.ts` on the numeric hot-path **must not** show
a new box/guard/probe. Typed receiver → `call`. Only `any` → 1 `DISPATCH_AUTO`.
CI compares the IR of the canonical benches (§7).

---

## 4. Fatal flaws of the current foundation (fix BEFORE the engine)

The user explicitly asked: *"if the rts-macro system has a flaw or something
similar, mention it."* I found **5 structural flaws + 3 papercuts.**
The engine logic (`resolve_method`/`emit_method_target`) is clean, but it rests
on three primitives — the **macro**, the **registry**, and the **`lookup`** — and
all three have holes that, if ignored, become a silent gap multiplied by
hundreds of methods.

### 4.1 CRITICAL — aggregation is still a hand-written array (stage 2 not done)

`GLOBAL_CLASS_SPECS` (63 entries) and `SPECS` (~48 entries) in
`crates/rts-codegen/src/abi/mod.rs` are **hand-maintained arrays**. The macro derives
the per-class `*_CLASS_SPEC`, but registering the class in the array is **manual**.

> **Why it's fatal for the engine:** the engine discovers methods by iterating the
> registry. Forgetting a `&…::ARRAY_CLASS_SPEC` line = the whole family vanishes
> from dispatch **with no compile error** — `resolve_method` returns `None`, the
> method falls into the fallback or blows up. "I declared the class and it doesn't
> work" with no clue. The quadruple-write is half-dead: declaring-a-method works
> INSIDE the class; registering the CLASS is still a manual copy.

**Fix (core-engine stage 2):** `linkme::distributed_slice`. Each
`#[rts_class]`/`#[rts_namespace]` does `#[distributed_slice(CLASS_REGISTRY)]` —
the array assembles itself at link time. `global_class_lookup` iterates the slice.
Forgetting becomes impossible: declared = registered.

### 4.2 CRITICAL — `jit.rs` still has 1104 hand-written `add_fn!` (stage 3 not done)

Every `__RTS_FN_*` extern must be registered **again** in the JIT symbol
map (`add_fn!("__RTS_FN_GL_STRING_X", path)`). Adding a method = `#[rts_method]`
(auto) **plus** a line in `jit.rs` (manual).

> **Why it's fatal:** forgetting the JIT line = the method works on AOT but gives a
> **missing-symbol / ACCESS_VIOLATION under `rts run`**. The engine multiplies the
> number of table-driven methods; each one is a chance to forget the `add_fn!`.
> Worse: it fails only on the JIT path, so it passes half the tests.

**Fix (stage 3):** `JITBuilder::symbol_lookup_fn` → `GetProcAddress(GetModuleHandle(NULL), name)`
(Win) / `dlsym(RTLD_DEFAULT, name)` (Unix). Every `__RTS_*` is `#[no_mangle]`
statically linked into `rts.exe`, already in the process's symbol table. Kills the
1104 lines. AOT caveat: ensure `#[used]`/export-list so the linker doesn't prune
unreferenced externs (test MSVC).

### 4.3 CRITICAL — `instance_method(name)` is first-by-name, NO arity

`global_class.rs:42`:
```rust
self.members.iter().find(|m| m.kind == InstanceMethod && m.name == name)
```

It is the **first by name**. If two rows share a `name` (overload
`indexOf/1` vs `indexOf/2`), **only the first is reachable through the table.** The
by-arity selection that exists is faked **at a single call-site**
(`mod.rs:1117`, `args.len()-1 == n_call_args`), not in the primitive.

> **Why it's fatal:** the table **structurally cannot hold
> overloads** today. String/Array live on overloads (`indexOf`, `slice`,
> `startsWith`, `splice`…). Migrating `builtins.rs` → rows is impossible while the
> lookup only sees the first row of each name.

**Fix — ✅ DONE (F2a `c2e1757f` + F2b `fe894ab9`):**
`GlobalClassSpec::resolve_instance_method(name, n_args)` with by-arity selection
inside the primitive (the `mod.rs:1117` logic became the reference), honoring
alias/variadic/optional-tail, first-by-name fallback. The dispatch call-sites
(`mod.rs:1117`, `mod.rs:2974`, `indirect.rs:36`) collapsed onto that primitive;
suite 1710/1710. (Read/type-inference sites keep using `instance_method`
first-by-name — arity irrelevant.)

### 4.4 CRITICAL — `NamespaceMember` is too thin for prototype semantics

The struct today: `name, kind, symbol, args:&[AbiType], returns, doc, ts_signature,
intrinsic, pure`. **Missing:** `aliases`, `variadic`, `default_args`,
`receiver_family` (or owner). Without those fields the macro **cannot express
as data**:

- aliases (`toLowerCase`|`toLocaleLowerCase`, `trimStart`|`trimLeft`, `includes`|`contains`) — today an OR-pattern hardcoded in the match.
- variadic (`push`/`concat`/`unshift`) — N calls from 1 source; the generic emitter zips 1:1 and **is wrong on mismatch** (`ns_call.rs:500`).
- default-args (`slice` end, `padStart` pad) — the emitter is wrong on too-few-args; default injection is **new emitter logic**, not something the runtime applies.

> **Why it's fatal:** without extending the struct + the macro, "migrating builtins → rows"
> covers only the fixed-arity-single-symbol subset. The rest stays hardcoded
> code — the engine is left half-done and the two resolvers stay alive.

**Fix — ✅ DONE (F1 `122e1392` + A1a/A2 `1af5bea0`):** `NamespaceMember`
gained `aliases: &[&str]`, `variadic: bool`, `default_args: &[DefaultArg]` (F1) +
`flags: MemberFlags` (A1a). Macro: `aliases`/`variadic` parsed in
`parse_class_member`; `readonly`/`static_field` become `MemberFlags`. Only the
`default_args` **value** syntax in the macro is missing (arrives in E3/E4 — the field
is already final). `resolve_instance_method` (F2a) already consumes `aliases`/`variadic`/`default_args`.

### 4.5 CRITICAL — the symbol is stringly-typed re-typed glue (bypasses the registry)

The macro derives `__RTS_FN_GL_<PREFIX>_<FN_IDENT.to_uppercase()>`. `builtins.rs`
**re-types the same literal** in each arm. There is no compile-time link between the
two. Rename the Rust fn → the derived symbol changes → the literal in `builtins.rs`
**still compiles** → silent breakage. Worse: there are emission paths
(`builtins.rs`) that **do not go through the registry**, violating the core-engine
safety thesis ("the registry is the single dispatch gate").

> **Why it's fatal for stability:** two sources of truth for every symbol
> (the spec row AND the literal). The `rts.d.ts` lint checks the spec against the
> generator, but **not** `builtins.rs`. Invisible drift.

**Fix:** **the engine emits only via `member.symbol`** — never a literal. As
each `builtins.rs` arm is deleted, the symbol comes from the row. Lint rule:
no `__RTS_FN_*` as a string literal outside a spec row or a
`symbol = "…"`. (Sub-papercut 4.5b: derivation by `to_uppercase()` is an unguarded
collision — `to_json`→`TO_JSON` and a hypothetical `toJson`→`TOJSON` diverge; two
distinct snake idents can collide in upper. Low probability, but add an
anti-collision `validate_symbol` in the aggregator.)

### 4.6 Papercuts (non-fatal, but fix alongside)

- **Null-Str default = `return 0`.** The macro's string-reconstruction prelude
  does `return <type-zero>` on null/invalid-UTF-8. For a `Handle`
  return, `0` is a potentially-valid/garbage handle, not "undefined". A string
  method with a null arg returns handle 0 → garbage/crash downstream. `on_null`
  exists but is opt-in. **Fix:** the default should be an explicit per-type
  error sentinel, not raw 0.
- **No spec↔extern cross-check.** Nothing guarantees every row has an extern with
  that symbol (`external` rows point at symbols of another namespace; a typo
  in `symbol="…"` only shows up at runtime). **Fix:** a build-time
  test that resolves every `member.symbol` against the symbol table.
- **No "every builtins arm has a row" gate.** During the migration, deleting an
  arm without the equivalent row = the method vanishes. **Fix:** extend the
  `rts.d.ts` lint to require a row before deleting an arm (divergence oracle, §6).

---

## 5. Stable and scalable architecture

Layers, from bottom (foundation) to top (engine). Each layer has **one** owner and
one contract; nothing skips a layer.

```
┌─ rts-macro ────────────────────────────────────────────────┐
│  #[rts_namespace] / #[rts_class] + #[rts_method] etc.        │
│  → derives: extern "C" + NamespaceMember (with aliases/      │
│    variadic/default_args) + distributed_slice registry entry │  FOUNDATION
└──────────────────────────────────────────────────────────────┘
┌─ rts-abi ──────────────────────────────────────────────────┐
│  NamespaceMember (extended) · GlobalClassSpec ·              │
│  resolve_member(name, n_args) (with arity) · Intrinsic       │
│  CLASS_REGISTRY / MEMBER_REGISTRY (linkme, self-assembled)   │
└──────────────────────────────────────────────────────────────┘
┌─ rts-codegen::engine (NEW) ────────────────────────────────┐
│  recv_kind(ctx, expr) -> RecvKind         (1 type point)    │
│  resolve_method(recv, name, n) -> MethodTarget  (1 lookup)  │  ENGINE
│  emit_method_target(ctx, target, recv, call)    (1 emitter) │
└──────────────────────────────────────────────────────────────┘
┌─ call-sites (calls/mod.rs, indirect.rs, members.rs) ───────┐
│  all call the engine; none has its own string-match         │  CONSUMERS
└──────────────────────────────────────────────────────────────┘
```

### 5.1 `RecvKind` — the missing receiver-type

```rust
enum RecvKind {
    Class(&'static str),  // "String","Number","Array","Map","Set","RegExp" + user classes
    UserClass(ClassId),   // user-defined class (virtual dispatch)
    ProtoInstance,        // runtime __proto__ map — preserves MAP_GET_CHAIN
    ObjectLiteral,        // preserves the #480 guard (user obj.add ≠ Set.add)
    Unknown,              // genuine `any` — arrow param, map-get, capture
}
```

`recv_kind` **consolidates in one point** what today is spread over ~12
`FnCtx` side-channels (`local_array_vars`, `local_string_vars`,
`local_class_ty`, …) + `lhs_static_class`. Same information, collected once.

**Honesty about types (non-negotiable):** static info is **sufficient** for
Tier-1 (statically known class) and Tier-2 (family via `ValTy::Bool`→Boolean,
numeric→Number, `: T[]`→Array) and **insufficient** for Tier-3 (opaque `Handle` —
arrow param, map-get/JSON.parse result). Tier-3 is **common** in
callback-heavy code, not a rare edge. The correct answer for Tier-3 is **never
guessing by order** (the source of today's bugs) — it is `RecvKind::Unknown` → `RuntimeAuto`.

### 5.2 `MethodTarget` — must cover ALL cases, not just builtins

```rust
enum MethodTarget {
    SymbolCall(&'static NamespaceMember),                 // ~majority: call symbol, marshal args
    InlineIr(MethodIntrinsic, &'static NamespaceMember),  // Bool.toString, array.at, charCodeAt-F64
    UserClassMethod { owner: ClassId, method, arity, virtual: bool }, // user class + operator overload
    VirtualDispatch { candidates: &[ClassId], method },   // override in a hierarchy
    Residual(ResidualKind),                               // variadic / regex-polymorphic / coercion
    RuntimeAuto(AutoKind),                                // Unknown receiver
}
```

> **Mistake to avoid (flagged by the review):** if `MethodTarget` is only
> `SymbolCall | InlineIr | RuntimeAuto`, the engine is a **builtins-only** gate —
> user-class methods and **operator overload** (`a+b → a.add(b)`,
> `operators.rs:347`) stay in a parallel resolver, and the engine **unifies
> nothing**. `UserClassMethod` + `VirtualDispatch` are mandatory from Step 1.

### 5.3 `MethodIntrinsic` — separate from `abi::Intrinsic`, do NOT reuse

> **Mistake to avoid (flagged by the review):** `abi::Intrinsic` is consumed by
> the MIR's `intrinsic_resolver_default` (an **exhaustive** match, no receiver
> model). Adding `BoolToString`/`ArrayAtNegIndex` there **breaks the MIR
> build** or pollutes a namespace-call enum with variants the MIR
> never reaches. Use a **`MethodIntrinsic`** enum local to the engine, with an
> emitter that receives the receiver. `lower_intrinsic` (namespace) stays untouched.

### 5.4 Residuals (stay as code, honestly)

Not everything becomes a row. These stay as explicit residual handlers, marked on the row:

- **Variadic** (`push`/`concat`/`unshift`): loop of N calls + spread (`VEC_EXTEND_FROM`).
- **Regex/callback-polymorphic** (`replace`/`split`/`match`/`matchAll`): branches on /regex/ vs string vs fn.
- **Coercion-construction** (bare `Array(…)`/`Number(…)`/`String(…)`): numeric-vs-handle branch.
- **Inline-IR** (`Bool.toString` select, negative `array.at`, `charCodeAt` F64): return a ValTy/sentinel different from declared.

**Honest payoff:** the engine deletes the **fixed-arity-single-symbol** arms (the
majority of the 182) + the 6 `*_AUTO` → 1 + the 4 try-order sites → 1. **Not** "half
of `builtins.rs` evaporated" — the residuals above remain. But they become a
**small, named** set, not 2258 lines of match.

### 5.5 `DISPATCH_AUTO` — design spike, not a freebie

The 6 `*_AUTO` have **heterogeneous** signatures (`SLICE_AUTO(u64,i64,i64)→u64`,
`CONCAT_AUTO(u64,i64)→i64`, sentinel `i64::MIN`). Collapsing them into one
`DISPATCH_AUTO(handle, method_id, argv)` requires an argv-boxing convention the
codebase **does not have**. **Do not delete the 6 typed ones** until the replacement
passes the ambiguous-handle suite with no SIGILL (honesty floor). Safe alternative:
keep AUTO per-shape but **generate them from the rows** (one source of truth, sigs
preserved).

---

## 6. Implementation order — foundation before engine

Each step ships on its own, **green build + green suite**. The golden rule: do not
build `resolve_method` on a foundation with the §4 holes.

> Canonical status: **§0.1**. The ✅ marks here are a detail view.

```
FOUNDATION (fixes §4 — prerequisite of the engine)
  F1  ✅ NamespaceMember += aliases, variadic, default_args (§4.4). (122e1392)
  F2  ✅ GlobalClassSpec::resolve_instance_method(name,n_args) arity-keyed (§4.3);
      dispatch call-sites collapsed. Suite 1710/1710. (c2e1757f + fe894ab9)
  F3  Unified Registry — Track A (no linkme): OnceLock<RwLock>, register_builtins()
      drains SPECS/GLOBAL_CLASS_SPECS; lookup reads the registry (§10.2). Track B (linkme,
      gated by F0) deletes the arrays afterwards (§4.1).
  F4  jit symbol_lookup_fn; shrink the 1104 add_fn! (§4.2). Test AOT MSVC.
  F5  Lint: no __RTS_FN_* literal outside a row; spec↔extern cross-check;
      null-Str default = explicit sentinel, not 0 (§4.5, §4.6).

ENGINE (on top of the solid foundation)
  E0  engine/ with RecvKind, MethodTarget (incl. UserClassMethod), MethodIntrinsic,
      resolve_method (wrapper over resolve_member), recv_kind (merges the
      side-channels into one point). Unit tests vs current behavior. Nobody calls it
      yet → suite untouched.
  E1  Route user-class + global-class through the engine (path that ALREADY works).
      + divergence oracle (runs engine-path AND old-path, traps on
      divergence over the suite). Visible and bounded regression.
  E2  Number (~27 arms, zero name collisions). DELETES lower_number_builtin only
      with a green suite. Template for the rest.
  E3  String (~50 arms, 34 rows already exist). Aliases/default_args via macro.
      Keeps only regex/matchAll/charCodeAt-F64 as residual.
  E4  Array/Map/Set: external specs over COLLECTIONS_*. Tier-3 → DISPATCH_AUTO
      (after the §5.5 spike). The *_AUTO vanish.
  E5  (LAST, highest risk) qualified-string ladder + bare-globals → registry.
      Includes #[rts_global] (generic global scope, §9.7) + generic bare-ident
      resolution. Extract the callee-shape classifier as a pure structural refactor
      FIRST (same order, just factored); fold only mechanical forwards.
  E6  (optional, perf) ValTy::Handle → Handle(HandleKind) mirroring HIR; deletes
      side-channels; integer class-id → vtable (kills the gc.string_eq chain). Engine
      becomes the MIR's shared resolver after HIR gains This + class-type.
```

**Why this order is the stable one:** F1-F5 make the table able to **hold** the
semantics (overloads, aliases, variadic) and **assemble itself** (linkme,
symbol-lookup). Only then do E0-E6 build the resolution on top. Building the engine
before F1-F3 = `resolve_method` over a table that has no overloads, doesn't
self-register and re-types symbols = exactly the silent-bug-multiplied
scenario the user fears.

---

## 7. Invariants (honesty + perf floor — never suspended)

1. **100% native.** Typed receiver (Tier-1/2) → direct `call <symbol>`, no
   box/probe. Only `any` (Tier-3) pays 1 tag-check — a cost that already exists today via
   `*_AUTO`. It is not interpretation.
2. **The numeric hot-path does not regress.** `rts ir bench/monte_carlo_pi.ts` etc. shows
   no new IR in the loop. CI compares.
3. **The row is the single source.** Every emitted symbol comes from `member.symbol`, never
   from a literal. No drift.
4. **Resolution is keyed, not ordered.** `(RecvKind, name, arity)` disambiguates
   — zero "first-to-return-Some-wins". Kills the whole class of ordering bugs
   (#311/#480, string-before-map SIGILL).
5. **Migration with an oracle.** No `builtins.rs` arm is deleted without a proven
   equivalent row (lint) and zero divergence over the suite (E1 oracle).
6. **Tier-3 never guesses.** A genuinely unknown receiver → `RuntimeAuto`,
   never a guessed family. False-Unknown = correct slow call; false-Class
   = crash. Conservative by construction.
7. **MIR stays routed-to-AST** in member/class until HIR gains This +
   class-type (E6). The engine is AST-only until then.

---

## 8. Relationship with `docs/specs/rts-core-engine.md`

| | core-engine.md | RTS_ENGINE.md (this) |
|---|---|---|
| Focus | **registration** (kills quadruple-write, object model, dynamic tier) | **dispatch** (kills the `builtins.rs` string-match) |
| Stage 1 (macro) | done (50 ns + 27 classes) | reuses |
| Stage 2/3 (linkme + jit) | planned, **not done** | **F3/F4 — prerequisite of this engine** |
| Stage 5 (vtable) | planned | **E6 — integer class-id** |

They are the same epic seen from two angles. This doc covers the half that
core-engine.md left implicit and adds the foundation audit (§4) that makes
the whole thing maintainable instead of a castle on hand-written arrays.

---

## 9. Rich authoring API in Rust (modules / classes / fns / consts / variables)

How to declare the ENTIRE RTS surface richly and extensibly from Rust,
so that adding a new family (like `node:`) is cheap. This is the
layer that materializes the §4 fixes.

### 9.1 Verdict: macro as the surface, linkme as the assembly

> **⚠️ Architect decision (supersedes this section for BUILTINS):** the project
> chose the **builder model** — the **`rts-engine`** crate (`fae05975`), a programmatic
> registry + fluent builder where the layers (`rts-std`/`rts-node`/
> `rts-browser`) register the surface, carrying the **native fn pointer**
> (which doubles as the JIT symbol) + the ABI sig. **`rts-macro` will be removed.** The
> trade below (the macro fuses extern+metadata) is real, but the builder accepts it
> in exchange for a **uniform builtin+external** path + the death of the hand-written arrays +
> the 1104 `add_fn!`. Signature ergonomics come back via the **declarative** macro
> `sig!` (not a proc-macro). The rest of this §9.1 remains as a record of the previous
> reasoning (valid under the macro+linkme model, now not chosen).

**[Previous reasoning — macro+linkme model, NOT chosen]** Proc-macro as the
authoring surface (`#[rts_module]`/`#[rts_class]`/`#[rts_fn]`/`#[rts_var]`),
**not** a runtime builder. Reason: only the macro fuses, in one declaration, (a) the
`#[no_mangle] extern "C"`, (b) the metadata derived from the signature (args/returns/ts),
(c) the StrPtr→ptr+len+UTF-8 prelude. A runtime builder would still require writing
each extern by hand, would move errors to startup, and would add a heap-registry — it solves
dynamic-modules at the cost of single-declaration.

**`linkme::distributed_slice` is the assembly** (not the surface). Each
`#[rts_module]`/`#[rts_class]` self-registers into a slice; the hand-written arrays
(`SPECS`, `GLOBAL_CLASS_SPECS`, `NODE_SPECS`) + the 1104 `add_fn!` die.
Preserved invariant: per-member stays `'static const`; **only the aggregate** becomes an
`OnceLock<Registry>` assembled at `rts.exe` startup — and since the codegen reads those
tables *inside* the running rts.exe (at lowering time), the heap belongs to the
**compiler**, not the emitted binary. Native purity intact.

> **Critical correction (review):** the AOT-safety reasoning is NOT about the user's
> AOT binary — `SPECS`/`GLOBAL_CLASS_SPECS` live in `rts.exe` (rts-codegen).
> The real risk is **rlib→bin transitivity**: does `rts.exe` retain the linkme
> entries registered in the `rts-runtime` rlib? That is linkme's fragile path,
> aggravated on **MSVC/COFF** (the primary target). → **F0 spike mandatory** (one
> trivial `#[distributed_slice]`, AOT MSVC build, assert the entry
> survives) BEFORE F3/F4. If it fails, Track A (below) survives without linkme.

### 9.2 Member model — modifiers as DATA (no new struct)

`NamespaceMember` gains **one** bitflag field + **three** MemberKinds. It unifies the
builtin world with the user-class world (which already models readonly/private/static/
setter in the AST's `ClassMeta` — `validate_visibility` members.rs:2216,
`field_is_readonly_in_hierarchy` :2269).

```rust
pub struct MemberFlags(u8);   // const-constructible, Copy
//   READONLY  — write = codegen error
//   STATIC    — static field (vs Constant read-once)
//   MUTABLE   — backing storage is an atomic var (rts_var let/var)
// (PRIVATE/PROTECTED: see 9.5 — do NOT become an enforced flag)

pub enum MemberKind {
    Function, Constant, Constructor, InstanceMethod, StaticMethod, InstanceGetter,
    InstanceSetter,  // NEW: fn(handle, value) -> void ; backs `inst.prop = v`
    VarGetter,       // NEW: fn() -> T ; backs reading `ns.v` of a mutable var
    VarSetter,       // NEW: fn(value) -> void ; backs `ns.v = e` ; absent => read-only
}
```

A settable builtin property = `InstanceGetter` + `InstanceSetter` with the same `name`
(replaces the string-keyed `pathname`/`lastIndex` branches at mod.rs:591-632).
A mutable module var = `VarGetter` + (optional) `VarSetter`.

> **Correction (review):** adding `MemberKind` variants **breaks every exhaustive
> `match`** over kind (signature.rs etc.) — it is the *forcing function* that reveals
> each consumption site. Treat it as a mechanical step (F2) before giving it meaning.

### 9.3 `#[rts_module("scheme:name")]` + ModuleScheme

Generalizes `#[rts_namespace]` to accept the full specifier (with `:`, scheme).
`NamespaceSpec` gains `scheme: &'static str`. A `ModuleScheme` (linkme slice)
makes "adding `node:`/`bun:`" data, not a branch:

```rust
pub struct ModuleScheme {
    pub prefix: &'static str,                                 // "rts" | "node" | "bun"
    pub exports_default: bool,
    pub resolve: fn(&str) -> Option<&'static NamespaceSpec>,
}
```
`builtin_module(spec)` (runtime.rs:21) becomes `split_once(':')` → find the scheme in the
slice → resolve. The bare-`rts` umbrella (RTS_EXPORTS) becomes **derived**
from the scheme=="rts" specs (kills the hand-list drift). Node stops having a
parallel `NodespaceSpec` — it becomes `NamespaceSpec{scheme:"node", alias_of:Some("fs")}`.

### 9.4 `#[rts_var(const|let|var, Type, default)]` + native write-path

User's choice: **process-wide atomic global** + `x.v` read and **native
write `x.v = 5`**. The macro expands:

```rust
#[rts_var(var, I64, default = 7)] static SEED;
// generates:
static __RTS_VAR_<NS>_SEED: AtomicI64 = AtomicI64::new(7);
#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_SEED_GET() -> i64 { …load(SeqCst) }
#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_SEED_SET(v: i64)  { …store(v,SeqCst) }
// + VarGetter member (+ VarSetter if let/var ; const => GET only + READONLY flag)
```
Type map: I64/Handle→AtomicI64, U64→AtomicU64, F64→AtomicU64(bits), Bool→
AtomicBool. Str cannot be a var (becomes Handle).

- **Read** (`ns.v`): add `VarGetter` to the `matches!(kind, Constant)`
  (members.rs:1006) — otherwise it reifies as a fn-handle. (NOT free, fixes the
  design.)
- **Write** (`ns.v = e`): ONE new arm at the top of `lower_assign_expr`
  (expressions/mod.rs:446, BEFORE the `MAP_SET` fallback at :1193 which today corrupts
  silently): resolves VarSetter via registry → `call <setter>(coerce(rhs))`;
  READONLY → hard error. The same arm covers builtin `InstanceSetter`.
- **GC root** (Handle var): emit a thunk only for `Type==Handle` into a
  `GC_VAR_ROOTS` slice, drained in main() **before the 1st GC tick** in JIT **AND** in the
  AOT `__RTS_MAIN` prologue (emit.rs today has **zero** global-root — a new path).

### 9.5 Enforcement vs cosmetic — readonly yes, private redesigned

- **readonly**: enforced at the write-site (the arm above errors before emitting a call).
  The macro stamps `READONLY` on **every getter-without-setter** — otherwise readonly-by-
  omission falls into the corrupted MAP_SET (review correction).
- **`#private`/protected on a builtin**: `validate_visibility` keys on
  `ctx.current_class`, which is **never** the builtin's name during a user
  call → enforcing is mechanically impossible. It does **not** become a dead flag that
  codegen pretends to read. Modeled as **"member not exposed to TS"** (the macro
  simply doesn't emit the public member). That is the honest representation: user
  TS code is never "inside" the builtin's Rust body.
- **d.ts**: the generator today does not emit class members (emit_types.rs:94/123).
  Surfacing modifiers is a follow-up decoupled from enforcement (byte-by-byte lint
  of `rts.d.ts` changes in a separate PR).

### 9.6 Tracks (separable — the risk does not block the value)

> Canonical status: **§0.1**.

```
Track A (low risk, WITHOUT linkme — do first)
  A1a ✅ member.rs: MemberFlags + InstanceSetter/VarGetter/VarSetter +
      GlobalClassSpec::instance_setter; exhaustive-match fallout handled. (1af5bea0)
      (NamespaceSpec.scheme is left for F3 — not needed yet.)
  A2  ✅ macro: #[rts_module("scheme:name")] ; #[rts_setter] ; readonly/static_field ;
      #[rts_var(kind,Type,default)] (atomic + GET/SET + member(s)). Test 6/6. (1af5bea0)
  A3  codegen: VarGetter read (members.rs:1006) ; VarSetter/InstanceSetter write-path arm
      (+ readonly hard-error) ; replace hardcoded pathname/lastIndex with InstanceSetter.
  A4  GC_VAR_ROOTS drain (JIT + AOT __RTS_MAIN), Handle only. Fixture: var read+write+readonly-reject.

Track B (gated by the F0 MSVC/COFF spike)
  F0  spike: 1 trivial #[distributed_slice], AOT MSVC build, assert rlib→bin survival.
  B1..B3  linkme: MODULE_SPECS/CLASS_SPECS/JIT_SYMBOLS + ModuleScheme ; kill the 3 arrays +
          add_fn! ; promote the drift-check to a release name-set assert. Each deleted array = 1 commit.

Redesign  PRIVATE/PROTECTED as not-exported (not an enforced flag).
```

The §7 invariants hold here: nothing becomes dead metadata (every modifier has a
codegen consumer or doesn't exist), 100% native, green suite per step.

### 9.7 `#[rts_global]` — generic global scope (kills the hardcoded ladders)

Today the **globals** (reachable without an `import`: `NaN`, `Infinity`, `undefined`,
`globalThis`, `isNaN`, `parseInt`, `parseFloat`, `isFinite`, `encode/decodeURIComponent`,
bare `Array()`/`Number()`/`String()`…) **are not generic** — they are string-match
hardcoded in **two** places in the codegen: value read in `basics.rs:390`
(`matches!(name,"NaN"|"Infinity"|"undefined")`) and calls in `lower_js_global_call`
(`mod.rs:3208`, a ladder of ~25 names). `global_this` is a facade
`#[rts_namespace(globalThis)]`. It is the same §10.7 problem, in bare scope.

`#[rts_global]` is the authoring surface that turns this into **data**: it declares
functions/constants/variables in **global scope** reusing `#[rts_fn]`/`#[rts_const]`/
`#[rts_var]`, registering the members in a `globals` table of the Registry (§10.2).

```rust
#[rts_global]                              // bare scope — no import
impl Globals {
    #[rts_const(F64)] const NaN: f64 = f64::NAN;
    #[rts_const(F64)] const Infinity: f64 = f64::INFINITY;
    #[rts_fn(ts = "isNaN(x: number): boolean")] fn is_nan(x: F64) -> Bool { x.is_nan() }
    #[rts_var(var, I64, default = 0)] static __debug_level;   // mutable global var
}
```

**Catch (the §4 trap):** the macro is trivial to add; what makes globals
generic is the **consumption** — codegen resolving bare idents via the registry instead
of the two ladders. That IS **E5 + Registry (F3)**. `#[rts_global]` without that
resolution = dead metadata. So it travels with E5.

**Generic bare-ident resolution** (the missing piece):
- Order (= JS, for shadowing): `local > param > user-fn > import-alias >
  registry.globals > error`. Globals **last** (`let NaN = 5` shadows).
  Mind #301 (var hoisting) + user globals.
- **READ** (`basics.rs`) and **CALL** (`lower_js_global_call`) consult
  `registry.globals`. The ladders shrink to **residual** (only those needing
  inline-IR/coercion: `parseInt`-radix, `isNaN`-coerce, `Array()`/`Number()`).
- **Mutable global var**: `#[rts_global] #[rts_var(var,…)]` → atomic + GET/SET
  (§9.4) + bare-ident assignment write-path (`g = x` → `VarSetter` in
  `registry.globals`).

Result: `globals/` (global_this + the ladders) becomes registry entries,
indistinguishable from namespace/class to the engine. Part of **E5** in the roadmap.

---

## 10. The NEW MODE — generic engine + native external modules

Extensibility **is not a separate "rts-plugin" subsystem**. It is what the engine
**is** once it becomes generic: codegen resolves everything through **one registry** and emits
a native `call`, without knowing whether the module was compiled-in (builtin) or loaded
from a `.dll`/`.so` (external). "Plugin" = just "external module registered in the same
engine". That is the **new mode**: nothing physical exposed inside the codegen.

### 10.1 The engine is already generic (the bet, verified)

`lower_ns_call_body` (`ns_call.rs:174`) **already** is an engine: given **any**
`&NamespaceMember`, it derives the Cranelift signature from `member.args`/`member.returns`
(via `lower_member`/`scalar_to_cl`) and emits
`module.declare_function(member.symbol, Import) + ins().call(fref)`. **Zero**
knowledge of which module/method is hardcoded there. The boundary is already typed
`extern "C"` + opaque `u64` handle (= the N-API model **without** the boxing layer).
The macro already fuses `#[no_mangle] extern "C"` + the row + the SPEC. Hence: an
external module's member lowers **byte-identical** to `io.print`.

### 10.2 Unified registry (builtin + external in the same place)

The three hand-written arrays (`SPECS`, `GLOBAL_CLASS_SPECS`, the 1104 `add_fn!`) become
**one** `OnceLock<RwLock<Registry>>` in rts-codegen — the **compiler's** heap
(rts.exe), read at lowering, **not** in the emitted binary (native purity intact).

```rust
struct Registry {
    modules: HashMap<&'static str, ModuleEntry>,   // "<scheme>:<name>"
    classes: HashMap<&'static str, ClassEntry>,
    jit_symbols: HashMap<&'static str, *const u8>, // symbol -> fn ptr (builtin AND external)
    _libs: Vec<Arc<PluginLib>>,                    // keeps each .dll/.so mapped
}
enum SpecOrigin { Builtin, External { lib: LibId } }
```
- **Builtin** populates at startup (Track A drains the const arrays; Track B linkme,
  gated by the F0 spike). `lookup`/`global_class_lookup` start reading the registry —
  **same call-sites**.
- **External** populates on `LoadLibrary`/`dlopen`: the host **copies/interns** the
  descriptors (string → `'static` arena), stores only the `fn_ptr` + interned
  symbol, and an `Arc<Library>` in `_libs` keeps the image alive.
- Codegen **does not distinguish** origin. `SpecOrigin` only decides the AOT path
  (builtin = static reloc; external = import slot) and diagnostics — it **never**
  branches the marshalling.

### 10.3 FROZEN external ABI (repr(C), versioned) — `rts-abi::c_plugin`

> **Verified:** `NamespaceMember`/`NamespaceSpec`/`GlobalClassSpec`/`AbiType`/
> `MemberKind`/`MemberFlags` **are not repr(C)** (only `js_error.rs` has
> `#[repr(u8)]`). They are Rust-layout with `&'static str`/`&[T]` (fat pointers) — **silent
> UB** if they cross a `.dll` compiled by another rustc version. That is why the
> external boundary is a **separate** repr(C) layer; the internal types stay.

```rust
// every external .dll/.so exports exactly this:
#[no_mangle] pub extern "C" fn rts_plugin_register(host: *const RtsHost,
                                                    reg: *const RtsRegistrar) -> i32;
pub const RTS_PLUGIN_ABI_VERSION: u32 = 1;
// AbiType/MemberKind/MemberFlags as FIXED u8/u32 codes — never the Rust discriminant.
#[repr(C)] struct RtsMemberDesc {
    name: *const u8, name_len: u64, kind: u8, flags: u32,
    args: *const u8, args_len: u64, returns: u8, variadic: u8,
    fn_ptr: *const c_void,                 // the REAL extern "C" pointer (load-bearing)
    symbol: *const u8, symbol_len: u64,    // canonical symbol (JIT name + AOT)
    ts_sig: *const u8, ts_sig_len: u64,
}
// + RtsHost (callbacks gc::alloc_string/buffer/vec/map, free_handle, gc_root_add/remove,
//   register_thread), RtsRegistrar (add_module/add_class), RtsModuleDesc, RtsClassDesc.
```
- `abi_version` is checked **before** reading any descriptor → a mismatch fails
  cleanly at load (log + skip), **never** a crash.
- Differential vs N-API: the external delivers **native typed pointers** that
  codegen `call`s directly — zero marshalling beyond the StrPtr/Handle builtins already
  have. No interpreter, no boxing.

### 10.4 Authoring — the SAME macro

The author writes the **same** `#[rts_module]`/`#[rts_class]` as a builtin, in a
`cdylib` crate, plus **one** line `rts_plugin_entry!{ modules=[...], classes=[...] }`
(macro-generated). A `cfg(rts_plugin)` arm in the macro emits the per-crate inventory +
the descriptors. The macro **derives the extern AND the descriptor from the same Rust
signature** — that is what closes the `fn_ptr` hole (§10.7).

### 10.5 JIT-first (real) vs AOT (heavy greenfield) — honest scope

- **JIT** (`rts run`/`test`): **real, days of work, zero codegen change.**
  Load the `.dll` in rts.exe before compiling, inject the `fn_ptr`s into
  `JITBuilder::symbol`/`symbol_lookup_fn`; `finalize_definitions()` resolves the
  external symbol just like `io.print`. Needs: `libloading` wrapper (~50 LOC,
  **there is zero dynamic-loading today**) + the registrar + 3 lines in
  `build_jit_module`.
- **AOT** (`rts compile`): **greenfield cliff, most of the engineering.**
  A static `Linkage::Import` **fails at link** on a symbol that only exists at
  runtime. Needs inventing from scratch: per-callee import slot +
  **`call_indirect`** (codegen has **ZERO** `call_indirect` today; replicate the
  StrPtr 2-slot expansion + `declare_value_needs_stack_map`) + a `dlopen`
  initializer before `__RTS_MAIN` + a static `dylib` namespace in the archive +
  a **trap-stub on an unfilled slot** (otherwise ACCESS_VIOLATION — honesty
  floor). MIR does no indirect (`TailCallIndirect` unimplemented). **NO-GO on
  shipping AOT** until that exists + is proven. JIT-first validates the ABI cheaply.

### 10.6 GC, threads, security — honest, no pretending

- **A GC root-set does NOT exist today** (`gc_root_add`/`remove` are **new** infra — there is
  only `Entry::Function::keep_alive`). A handle the external holds across
  await/callback/thread without pinning gets swept (GC_TICK_INTERVAL=256). The
  persistent root-set must be built before exposing the contract.
- **Cross-thread scan is Windows-only** (`thread_registry` — SuspendThread; Linux/
  macOS = no-op, main thread only). `register_thread` does **not** make an external's
  thread handle safe outside Windows. Build the portable scan OR scope
  external-with-threads to Windows + document.
- The external **never** allocates its own handle — only via `RtsHost.alloc_*` (shared
  HandleTable). The handle is opaque (the gen|slot|shard layout is not stable).
- **Security = honesty, not sandbox.** Loading a native `.dll` = arbitrary code
  with full process privilege (same as `.node` N-API). Defenses are
  **integrity**: SHA256-pin in the lockfile, verify before `LoadLibrary`,
  **load only from the manifest** (`rts.json` `rtsPlugins`, never auto-discovery).
  `import "plugin:foo"` without a manifest entry = **hard compile error**
  (fail closed). Disable/last-priority the JIT dlsym fallback for externals +
  a reserved symbol namespace `__RTS_FN_PLUGIN_*` (anti-shadowing). Document:
  declaring a plugin = authorizing code execution. No claiming containment.

### 10.7 builtins.rs is a PREREQUISITE of the thesis, not a footnote

"The registry is the single gate" and "codegen without hardcode" **are only true after**
E2-E4 (§4): today String/Array/Map/Set/Number/console/RegExp dispatch via
string-match in `builtins.rs` (2258 lines, ~182 arms), with symbols and
signatures re-typed, **outside the registry**. Array/Map/Set **don't even have a spec**.
So an external **cannot** extend receiver-method dispatch the way
`String.indexOf` does — because `String.indexOf` **also** doesn't go through the
registry. Until `builtins.rs` is drained into rows, externals are
first-class only on the **namespace + global-class** path (already generic), not the
type-method one. **Additional must-fixes** before externals ship: `fn_ptr`
by-honor → **v1 macro-authored only** (the macro derives extern+descriptor from 1
signature) + validate AbiType codes at register; **pin Bool=i64** (signature.rs
lowers Bool→I64; the ty.rs doc lies "i8" → fix); F2 arity-keyed **before**
external overloads; move the 2 symbol-switches of `ns_call.rs` (:272 spawn-bitcast,
:314 ambiguous-ret) to `MemberFlags` (RAW_BITS_ARG/AMBIGUOUS_RET) — otherwise emit
isn't data-driven and an external can't declare those.

### 10.8 Order (composes with F/E/A — not a fork)

> Canonical status: **§0.1** (the `S*` here map to the `F*`/`X*` there). `S1`=F1✅+A1a✅,
> `S2`=F2✅, `S3`=F3, `S4`=F4, `S5`=X1, `S6`=X2, `S7`=X3, `S8`=X4, `S9`=X5.

```
JIT-first (real, days):
  S1  ✅ member.rs frozen surface (F1/A1a done) + NamespaceSpec.scheme (left for F3).
  S2  ✅ F2 resolve_instance_method arity-keyed (overload prerequisite).
  S3  F3 Track A: unified Registry; register_builtins() drains the const arrays. lookup reads the registry.
  S4  F4 jit symbol_lookup_fn (builtins via GetProcAddress/dlsym + registry.jit_symbols); shrinks add_fn!.
  S5  dynamic-loading: libloading wrapper (LoadLibrary/dlopen/Mach-O). ~50 LOC. Tested with a throwaway .dll.
  S6  freeze rts-abi::c_plugin (repr(C) + RTS_PLUGIN_ABI_VERSION + fixed u8 codes). Additive.
  S7  JIT loader: manifest + SHA256 + LoadLibrary + RtsHost + register + intern in the Registry. "plugin:" scheme.
      → 1st external module runs under `rts run`. MILESTONE.
  S8  rts_plugin_entry!{} + cfg(rts_plugin) arm in the macro. Reference plugin (crc32) + test.
AOT (greenfield, gated):
  S9  call-site fork by SpecOrigin → import-slot + call_indirect (StrPtr 2-slot + stack map),
      dlopen initializer before __RTS_MAIN, static dylib namespace, trap-stub. NO-GO until proven.
Parallel (prerequisite of the genericity thesis):
  E2-E4  drain builtins.rs into rows + gc_root_set + portable cross-thread scan.
```

Each step green build+suite. JIT-first reuses the already-generic named path (no
codegen change); AOT is an entire new subsystem (a dynamic-loading capability the
project has zero of today) — don't let JIT success imply AOT is close.

---

## 11. Verification & CI gates (how each step proves itself)

Consolidation of the verification contracts scattered across the sections. **No step
merges without its gate.** The honesty floor (§7) is verifiable, not trust.

| Step | Gate (failure = blocks merge) |
|------|-------------------------------|
| **F1/A1a/A2** ✅ | additive: `cargo test --lib` + macro `derive.rs` (rts_var atomic round-trip + flags); TS suite untouched (default-empty fields). |
| **F2** ✅ | `cargo test -p rts-abi` (overload/alias/variadic/optional-tail) + TS suite 1710/1710 (dispatch change). |
| **F3** | **parity-count**: `register_builtins()` produces exactly the same N entries as the const arrays (startup assert). Build the Registry 2× → identical `(scheme,name)` order. `rts.d.ts` **byte-identical** (existing CI lint). |
| **F4** | **name-set assert (release, permanent)**: every `member.symbol` that has an extern (non-`alias_of`/`external`/`intrinsic`) ∈ `JIT_SYMBOLS`. **Do not delete** the old drift-check — promote it. `rts run` + `rts compile` both green (test export on MSVC). |
| **F0** | trivial `#[distributed_slice]` spike in rts-runtime, **AOT MSVC** build, assert rlib→bin survival. Failed → Track A is the permanent floor; does not block plugins. |
| **E1** | **divergence oracle**: runs engine-path AND old-path side-by-side over the entire suite; **traps on any divergence**. Catches latent ordering dependencies (#311/#480, string-before-map SIGILL). |
| **E2-E4** | per-family, **one per commit**, full suite between each. **lint**: before deleting a `builtins.rs` arm, require the equivalent row (matching symbol + arg-ABI). |
| **perf (every codegen step)** | `rts ir bench/{monte_carlo_pi,pi_machin}.ts` **diff-zero** in the hot loop (no new box/guard/probe). CI compares against golden IR. |
| **A3/A4** | TS fixture: var read + write (`x.v=5`) + readonly-reject (compile error). Handle-var survives a forced GC cycle (GC_TICK_INTERVAL=256). |
| **X2/X3** | reference plugin (crc32) loads + calls under `rts run`. ABI-version mismatch → clean failure (log+skip), **never crash**. `node:fs`/alias build with **zero duplicate symbols**. |
| **X5 (AOT)** | unfilled slot → **trap-stub with a message** ("plugin X member Y not loaded"), **never** ACCESS_VIOLATION. A program that runs under `rts run` and uses a plugin → **compile-time** error under `rts compile` if the plugin doesn't match (fail closed, never at link/trap). |
| **security (X*)** | `import "plugin:foo"` without a manifest entry = **compile error**. SHA256 verified **before** `LoadLibrary`. A plugin symbol outside the `__RTS_FN_PLUGIN_*` namespace → rejected at insert. |

---

## 12. Glossary

- **RecvKind** — the receiver-type resolved at one point (`Class`/`UserClass`/
  `ProtoInstance`/`ObjectLiteral`/`Unknown`), replacing the ~12 `FnCtx`
  side-channels. The resolution key. (§5.1)
- **MethodTarget** — the result of `resolve_method`: `SymbolCall` | `InlineIr` |
  `UserClassMethod` | `VirtualDispatch` | `Residual` | `RuntimeAuto`. What the
  emitter consumes. (§5.2)
- **MethodIntrinsic** — an enum **local to the engine** (≠ `abi::Intrinsic`, which is
  MIR-shared) for the few inline-IR arms that need the receiver. (§5.3)
- **Residual** — a method that does **not** become a row (variadic, regex-polymorphic,
  coercion-construction); an explicit named handler. (§5.4)
- **RuntimeAuto / DISPATCH_AUTO** — the path for an `Unknown` receiver (Tier-3): 1
  runtime tag-check + a native `call`. **Not** interpretation. (§3.3, §5.5)
- **Tier 1/2/3** — how much the compiler knows about the receiver: static class (1),
  family via ValTy+signals (2), opaque `Handle`/`any` (3 → RuntimeAuto). (§5.1)
- **Track A / Track B** — A = Registry without linkme (drains the arrays at startup,
  permanent floor). B = linkme self-assembles + deletes the arrays (gated by F0). (§9.1, §10.2)
- **SpecOrigin** — `Builtin` | `External{lib}` in the Registry. Metadata for the AOT
  path (static reloc vs import-slot) + diagnostics, it **never** branches the
  marshalling. (§10.2)
- **Registry** — `OnceLock<RwLock<Registry>>` in rts-codegen (the compiler's heap,
  not the emitted binary's). Single source; builtins + externals. (§10.2)
- **c_plugin / RtsHost / RtsRegistrar** — the **frozen** repr(C) ABI an external
  `.dll`/`.so` exposes; `RTS_PLUGIN_ABI_VERSION`. (§10.3)
- **ModuleScheme** — an import family (`rts:`/`node:`/`plugin:`/custom) as
  **data** (slice), not a hardcoded branch. (§9.3)
- **`#[rts_global]`** — bare-scope (no import) authoring: `NaN`/`isNaN`/global
  var → registry entries. (§9.7)
- **divergence oracle** — runs engine-path + old-path side-by-side,
  traps on divergence. E1 gate. (§11)

---

## 13. Out of scope / non-goals

What the engine does **NOT** change (to not inflate the blast radius):

- **JS semantics do not change.** The engine is a *dispatch* refactor, not a
  behavior one. All observable output stays identical (gate: suite + oracle).
- **The GC model does not change** — except the **new root-set** (`gc_root_add/remove`)
  that A4/X* require. Precise mark+sweep, stack maps, shards: untouched.
- **MIR stays routed-to-AST** in member/class/this until HIR gains `This` +
  class-type (E6). The engine is AST-only until then — not a regression, the current
  state preserved.
- **No plugin sandbox.** Loading a `.dll`/`.so` = arbitrary native code,
  full privilege (same as `.node`/N-API). Defense is integrity (SHA256 +
  manifest-only), not containment. No claiming otherwise. (§10.6)
- **No plugin hot-reload** in v1 — the `Arc<Library>` lives for the whole process; `'static`
  rows point at interned strings, the lib never drops mid-lowering.
- **Plugin AOT is gated** (X5) — do not ship until import-slot + call_indirect +
  trap-stub are proven. JIT-first validates the ABI cheaply.
- **`default_args` value-in-the-macro** arrives in E3/E4 — the field is already final (F1); only
  the default-injection syntax in the emitter is missing.
- **Builtin PRIVATE/PROTECTED does not become an enforced flag** — modeled as
  "member not exposed to TS" (codegen cannot check `ctx.current_class`
  against the builtin's name). (§9.5)

---

> **The end.** This doc is the canonical spec of the dispatch-half + new mode of the
> rts-engine. Live status in §0.1; detailed order in §6/§9.6/§10.8; proofs in
> §11. Changed the code and the rule lied? Update the doc in the same PR (RULE #0).
