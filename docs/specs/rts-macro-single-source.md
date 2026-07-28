# `rts-macro` as the single source of truth for symbols

Status: **DIRECTION APPROVED (owner, 2026-07-27). Phased implementation in progress.**

## The goal

`rts-macro` becomes the ONE place a runtime symbol is declared. Today a single
symbol is spelled out in four independent places; after this it is spelled once,
and everything else is derived.

Today, for `__rtsadp_obj_get`:

| # | Where | What it says |
|---|---|---|
| 1 | `rts-runtime/src/adapters/value/objops.rs` | the `#[unsafe(no_mangle)] pub extern "C" fn` definition |
| 2 | `rts-codegen-new/src/adapter_symbols/list_b.rs` | `(name, fn_ptr)` for the JIT symbol table |
| 3 | `rts-codegen-new/src/value/abi_sig.rs` | `SymSig { params, ret }` for the Cranelift signature |
| 4 | ~25 lowering files | the literal string `"__rtsadp_obj_get"` at each call site |

Nothing checks that (3) agrees with (1). `abi_sig.rs`'s own doc-comment says it
exists because *"Mis-marshaling a single-slot string where the ABI expects two →
SIGILL"* — so the table that exists to prevent a SIGILL is itself hand-synced,
unguarded, against the definition it mirrors. That is the concrete bug class this
work removes.

## The target surface

### Modules — closure-scoped sub-declarations

```rust
e.module("node:fs", |m| {
    m.alias("fs");            // bare `fs` also resolves
    m.namespace("fs");        // JS namespace object — DISTINCT from the module specifier
    m.registry(READ_FILE);    // a free function
    m.registry(STATS);        // a class
});
```

`module` names the IMPORT specifier (`import … from "node:fs"`). `namespace`
names a JS-visible namespace OBJECT (`fs.readFile(...)` reachable without an
import). They are independent: a module may have no namespace, and a namespace
may be published for a module under a different name.

The closure form replaces the fluent `.done()`-terminated chain because a chain
cannot express nesting — every sub-declaration today has to be a method on the
same builder, which is why `alias`/`member` live at one flat level.

### Globals — no module, no namespace

```rust
e.global(|g| {
    g.registry(STRING_CLASS);
    g.registry(PARSE_INT);
});
```

For everything reachable as a bare global (`String`, `parseInt`, `globalThis.*`)
with no import and no namespace qualifier.

### Free functions

```rust
#[rtse::function("readFile")]
fn read_file(path: &str, encoding: Poly) -> Handle { … }
```

Today every generated symbol must be a member of a class (`gen_impl` always emits
`e.class(#name)`). A free function has no receiver and no class — this is the
first genuinely new `MemberKind`.

### Well-known symbols — `#[rtse::symbol("iterator")]`

```rust
#[rtse::class("Map")]
impl MapClass {
    #[rtse::symbol("iterator")]
    fn iter(recv: Poly) -> Handle { … }     // `for (const x of map)` finds this
}
```

This is what makes a class implemented **in Rust** participate in the JS
iteration protocol. Today a class can only be iterable if it is written as `.ts`
with a literal `[Symbol.iterator]()` method, which is why `Map`/`Set` live in
`rts-shared/src/stdlib/map_set.ts` rather than in Rust — the protocol key has no
declarable form on the Rust side.

`#[rtse::symbol]` registers the member under the well-known symbol key instead of
a string name, so `for…of`, spread, and destructuring resolve it through the same
protocol path they use for a `.ts` class. Coverage should include at least
`iterator`, `asyncIterator`, `toPrimitive`, `hasInstance` and `toStringTag` —
those are the ones the engine's own paths already consult.

Consequence worth stating: once this exists, `Map`/`Set` can move from `.ts` to
Rust. The `.ts` placement was a workaround for the missing declaration form, not
a design preference — see the note in `CLAUDE.md` about collections being `.ts`
"because they need arbitrary key/value types the i64 Rust backend can't hold",
which the `PolyValue` containers have since made false.

### Classes, including the call-without-`new` form

```rust
#[rtse::class("String")]
impl StringClass {
    #[rtse::constructor]
    fn new(value: Poly) -> Handle { … }     // `new String(x)` → wrapper object

    #[rtse::functioncall]
    fn call(value: Poly) -> Poly { … }      // `String(x)` → primitive coercion
}
```

`#[rtse::functioncall]` is the missing half of the JS callable-class protocol.
`String(x)` and `new String(x)` are different operations with different results
(primitive vs wrapper object), and today the distinction is **hardcoded in the
engine**: `crates/rts-codegen-new/src/front/run/globals.rs:60-105` special-cases
`"String"`, `"Number"` and `"Boolean"` by name and routes them to `.ts` prelude
factories (`StringFactory`, `NumberFactory`, `BooleanFactory`). That is a
primordial-class name hardcoded in codegen control flow — the doctrine's own
anti-pattern — and it exists only because the class had no way to declare "this
is what I do when called without `new`".

## The hard requirement: `SymbolDesc` must be a compile-time constant

There are two kinds of consumer, and they need the symbol at different times:

- **Registry-dispatched members** (`Date.now`, `url.parse`): the codegen resolves
  them at RUNTIME through `resolve_method`, and `adapter_symbols` harvests the
  fn-ptr from the Registry. Name discovered late. Already works.
- **Codegen-emitted intrinsics** (`obj_get`, `arr_push`, `VEC_GET`): the codegen
  writes the name as a literal in its OWN source and builds the Cranelift
  `Signature` during lowering. It needs name + ABI shape at **its own compile
  time**, before any runtime exists.

So the macro must emit, next to the extern wrapper, a `const` the codegen can
`use`:

```rust
// generated
pub const OBJ_GET: SymbolDesc = SymbolDesc {
    name: "__rtsadp_obj_get",
    params: &[AbiType::U64, AbiType::U64],
    ret: AbiType::U64,
};
```

and the lowering becomes `self.call_runtime(module, OBJ_GET, &[a, b])` instead of
`self.call_runtime(module, "__rtsadp_obj_get", &[a, b])`.

This is what collapses the four tables into one: (2) and (3) are derived from the
same const, and (4) references it instead of retyping a string. The SIGILL drift
class disappears by construction rather than by discipline.

### Why `.registry(CONST)` and not `.registry(read_file)`

The owner's sketch passes the item itself. Rust does not permit it: a `fn` item's
type cannot be named on stable, so no trait can be implemented for it, and a unit
struct sharing the function's name collides in the value namespace. The macro
therefore emits a `const` with a derived screaming-case name and `registry` takes
that. Same single-source property, one extra identifier.

## What the macro must gain

Audited against `crates/rts-macro/src/lib.rs` (1414 lines) as of 2026-07-27:

| Capability | Today | Needed for |
|---|---|---|
| `#[rtse::function]` (free fn, no class) | absent — `gen_impl` always emits `e.class(…)` | `node:fs` style module functions |
| `#[rtse::functioncall]` | absent | `String(x)` vs `new String(x)`; unhardcodes `globals.rs` |
| `SymbolDesc` const export | absent | the whole single-source property |
| Closure-scoped builder (`module`/`global`) | absent — flat fluent chain + `.done()` | nesting, `namespace` distinct from module |
| `namespace` distinct from module | `ns()` is the module spec | JS namespace objects |
| Variadic | `variadic: false` hardcoded (L1225, L1311, L1345) | `splice`, `Math.max`, `console.log` |
| `NativeEmit` | `emit: None` hardcoded (L1229, L1315, L1349) | inline-IR fast path without the engine naming anything |
| Real default-arg semantics | positional index | `null`/`undefined`/`NaN`/specific-string defaults |
| Callback params | absent | `arr_map`/`filter`/`reduce` — invoke a handle as a function |
| `Vec<Poly>` / iterator returns | only `String`/`Handle`/tuple-of-`String` | `arr_entries`/`values`/`keys` |
| `#[rtse::symbol("iterator")]` | inert marker only | a Rust class joining the iteration protocol — unblocks `Map`/`Set` in Rust |

### The default-argument gap, specifically

The current mechanism keys an optional argument by POSITION, which cannot
distinguish "argument absent" from an argument explicitly passed as `undefined`,
`null`, or `NaN`, and cannot express a default that is a specific string
(`encoding = "utf8"`). The fix is for the argument descriptor to carry a
`PolyValue` rather than an index — then every JS default is the same mechanism,
because `undefined` and `"utf8"` are both just values.

## Phases

Each phase is independently shippable and independently revertible.

**F0 — `SymbolDesc` + the drift guard.** Emit the const from the existing macro
paths; make `abi_sig.rs` consume it instead of hand-typing; add a test asserting
every entry agrees with its definition. Purely additive: nothing changes shape,
the SIGILL exposure closes.

**F1 — closure-scoped builder.** `e.module(spec, |m| …)` and `e.global(|g| …)`
alongside the existing fluent form. Old form keeps working; migrate call sites
incrementally.

**F2 — `#[rtse::function]` + `.registry()`.** Free functions with no class. Pilot
on one small module before `node:fs`.

**F3 — `namespace` distinct from module.** JS namespace objects.

**F4 — `#[rtse::functioncall]`.** Then delete the `String`/`Number`/`Boolean`
special-casing from `globals.rs` — the first hardcoded primordial name removed
from codegen control flow by this work.

**F5 — default-arg semantics on `PolyValue`.** Unblocks the specific-string and
nullish defaults.

**F5b — `#[rtse::symbol(...)]`.** DONE (2026-07-27), corrected from the original
text here: `#[rtse::symbol("iterator")]` (a string) is a REGISTRY symbol —
`Symbol.for("iterator")`, string-keyed — NOT the well-known iteration protocol.
`Symbol.for("iterator")` and `Symbol.iterator` are different JS values (a class
declaring `[Symbol.for('iterator')]` is not iterable by `for…of`); the original
wording here conflated them. The well-known form is
`#[rtse::symbol(Symbol::iterator)]` — a Rust PATH into
`rts_primitives::symbol::wellknown::Symbol`'s associated consts, emitted
verbatim into the generated member so a typo is a compile error, not a silent
runtime miss (the same drift-to-compile-error trade `#[rtse::abi]` made for
signatures). Both forms key the member `@@sym:<name>` / `@@<key>` — the same
`@@`-prefixed spelling the engine's computed-key desugar already uses for a
`.ts` class's literal `[Symbol.iterator]()`
(`front/run/desugar/objmethod/collect.rs`), so a Rust-declared member and a
`.ts`-declared member resolve through one protocol path. The well-known key is
NOT derived syntactically from the Rust path's last segment — it is read at
registration time from `WellKnown::member_key()` (`#path.member_key()`, a
runtime expression, not a compile-time string literal), which formats
`@@<WellKnown.key>`. This matters because a Rust ident can differ from its JS
name when the JS name is a Rust keyword: `Symbol::matcher` (Rust) has
`key: "match"`, so it registers as `@@match`, not the wrong `@@matcher` a
syntactic ident-to-key derivation would produce. `WellKnown.key` is the single
source of the JS key; the macro never re-derives it. This makes a
Rust-implemented class iterable; `Map`/`Set` become candidates to move out of
`rts-shared/src/stdlib/map_set.ts` into Rust.

**F6 — variadic, then `NativeEmit`.** `NativeEmit` last of these two because it
is what lets an intrinsic have an inline-IR path with the engine naming nothing.

**F7 — drain hand-written `extern "C"`, group by group**, each with the coverage
guard `adapter_symbols` already has. The object/array family (~120 symbols) goes
LAST: biggest, hottest, and it depends on F0 and F6.

## Non-goals

- This does not change how the codegen RESOLVES intrinsics. `obj_get` stays a
  direct call emitted by the lowering; only its NAME and SIGNATURE stop being
  hand-typed. Routing intrinsics through Registry name-dispatch would change
  resolution semantics and is explicitly not part of this.
- This does not touch object storage layout. That is a separate question with its
  own (currently unresolved) analysis.

## Verification

- F0 ships with a test asserting `abi_sig` entries match the macro-emitted consts
  for every migrated symbol — the guard that does not exist today.
- Every phase runs `cargo test --release -p rts-codegen-new --lib` and the TS
  suite; the baseline is 822/7 unit and 729/739 files with 3 genuinely-failing
  (`function_global`, `node_fs_basic`, `node_string_decoder_full`) plus one hang
  (`node_querystring_full`).
- `bash scripts/read_before_commit.sh` before every engine commit.
