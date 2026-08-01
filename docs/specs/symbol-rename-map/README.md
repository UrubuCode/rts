# Symbol rename map — `__RTS_FN_*` → `__rtsm_` / `__rtsn_`

Input for phase **N5** of [`../../../RTS_ORGANIZATION.md`](../../../RTS_ORGANIZATION.md)
and step 6 of [`../no-mangle-drain.md`](../no-mangle-drain.md).

## What this is

**423** legacy symbols still to rename. Started at 626; **74 rows dropped** on
2026-07-31 when N7 deleted the symbols they named (see the N7 section), and
**129 rows EXECUTED and retired** the same day (`dom` 102, `input`+`render` 27).
Each remaining row is
TSV:

```
<legacy symbol>	<body of #[rtse::abi(...)]>	<resulting linker symbol>
```

Produced on 2026-07-31 by 10 parallel agents, one per disjoint area. **The
`<value>` was read off the REAL registration site** (`.member(func("nomeJS",
"__RTS_...", …))` / `Member { name: …, symbol: … }` / `e.class(…)`), never
derived from the UPPER_SNAKE spelling — that spelling is ambiguous and a regex
over it produces wrong names. Measured examples where a regex would have broken
silently:

| legacy | real JS name | a regex would have produced |
|---|---|---|
| `__RTS_FN_NS_PROTOBUF_READER_FIELD_NUM` | `lastFieldNumber` | `readerFieldNum` |
| `__RTS_FN_NS_PROTOBUF_WRITER_TAG` | `writeTag` | `writerTag` |
| `__RTS_FN_NS_INPUT_TEXT` | `textInput` | `text` |
| `__RTS_FN_NODE_EE_PREPEND` | `prependListener` | `prepend` |
| `__RTS_FN_GL_DATAVIEW_SET_INT32_LE` | `setInt32` + `overload = "LE"` | collides with the base form |

15 of the 23 `protobuf` symbols turned out to be methods of the `ProtoWriter` /
`ProtoReader` CLASSES, not members of the namespace.

## Conventions applied

* Scope segment keeps its case (landed in `98d8d385`): `__rtsm_global_File_arrayBuffer`.
* `<value>` is the JS spelling verbatim.
* Overloads use `overload = "…"` (`rts_abi::scope::with_overload`), so
  `setInt32` / `setInt32_LE` do not collide.
* **`__rtsa_` / `Scope::Abi` was DELETED on 2026-07-31** (`RTS_ORGANIZATION.md`
  §4): it named zero symbols, and everything it was meant to carry is "the
  Cranelift IR cannot express this" — which is what `__rtsn_` already means. The
  50 rows first written as `abi, value = "…"` → `__rtsa_*` (they predate the
  decision) have been re-pointed at `native, value = "…"` → `__rtsn_*`:
  `primitives_std` 22, `input_render_engine` 15, `math_buffer` 9, `shared_b` 3,
  `node_rest` 1.

Distribution: 576 `__rtsm_`, 50 `__rtsn_`, 0 `__rtsa_`.
Verified: zero duplicates within the map, zero collisions against the baked table
(re-checked after the `__rtsn_` re-point).

## MISSING — two areas still to map

Both were produced by an agent and lost to a scratchpad write that failed; the
data was never committed. Re-run one agent each (cheap, ~2 min):

| area | symbols | note |
|---|---:|---|
| `crates/rts-shared/src/collections/{vec,map}.rs` | 125 | **only 26 are registered** — the other 99 have no JS name at all (no `.member(…)` anywhere). 104 of the 125 have no external consumer either. |
| `crates/rts-node/src/{net,dgram}/` | 111 | all methods of the `DgramSocket` / `Socket` / `Server` CLASSES (`global = "…"`), not of the module. Heavy arity-overload use (`send_1`…`send_6`). |

## Findings worth carrying forward

* **~150 symbols are not Registry members at all.** The `Function` / `Error` /
  `Reflect` / `Response` / `Request` class-spec builders were deleted in
  DRAIN_MOTOR; that dispatch is hardcoded in the front-end. Owner decision
  2026-07-31: map them as `__rtsm_global_<Class>_<js>` anyway — they WILL get a
  Registry row so JS can see them.
* **The consumerless symbols were DELETED, not renamed — N7 ran first
  (2026-07-31).** The earlier plan here ("rename now, audit for deletion
  separately") had the order backwards, and the reason is not bookkeeping: a row
  mapping a dead `__RTS_FN_*` to `__rtsm_global_<Class>_<js>` would have given it
  a Registry name and **resurrected it as TS surface** rather than deleting it.
  The `Error`, `Reflect`, fetch `Response`/`Request` and console-override
  families were all in that shape.

  Measured: **173 dead, not ~150**, and the audit's "~104 of `collections`" was
  wrong in a way that breaks the link — 119 exist, **75 dead, 44 live**. Full
  list and the prefix traps: [`../dead-symbols-n7.md`](../dead-symbols-n7.md).

  **166 symbols were deleted**; this map's rows were then filtered mechanically
  against the re-baked table, dropping every row whose legacy name no longer
  exists: `primitives_std` 59, `math_buffer` 7, `input_render_engine` 4,
  `shared_a` 4 — **74 total**. That filter, not a hand review, is what makes the
  remaining 552 a bijection with zero orphans.
* `__RTS_FN_NS_GC_*` are NOT members of the `gc` namespace: `e.ns("gc")` exposes
  only `collect` / `live_count`, both already `__rtsn_*`.

## Execution record

### `dom` — DONE (2026-07-31, 102 symbols)

The first area executed, and picked because it is the cleanest: **zero consumers
outside `rts-dom`**, all 463 occurrences in one file (`crates/rts-dom/src/abi.rs`).

Mechanism, per row, and it is worth copying:

1. rewrite the attribute `#[rtse::abi("__RTS_FN_NS_DOM_X")]` →
   `#[rtse::abi(module = "dom", value = "jsName")]`;
2. substitute the OLD NAME everywhere else in the file with the new symbol —
   that one substitution covers the Rust fn name, the Registry `symbol` string,
   the `X as *const u8` address and any internal call, because in this
   convention **the Rust item name IS the symbol**.

Two things that would have broken it:

- **Process rows LONGEST-NAME-FIRST.** `__RTS_FN_NS_DOM_ADD_LISTENER` is a
  prefix of `__RTS_FN_NS_DOM_ADD_LISTENER_CB`; a shortest-first or unordered
  sweep corrupts the longer name. Word boundaries alone are not enough.
- **Check the `value` against the name ALREADY REGISTERED.** The symbol is
  invisible to TypeScript, but `value` also names the Registry member. Here the
  registration already read `"addListener"` and the map said
  `value = "addListener"`, so the rename was symbol-only. Had they differed, the
  rename would silently have changed the JS API.

**The map had a systematic gap: `macro_rules!`-generated symbols.** It was built
by scanning `#[rtse::abi("…")]` textually, so the five `nav_fn!` navigation
getters (`firstElementChild`, `lastElementChild`, `nextElementSibling`,
`previousElementSibling`, `parentElement`) were absent — 102 of this file's 107.
They surfaced only as a leftover grep. This is the same blindness that hid the
eight `ta_ctor!` TypedArray constructors from `rts-symbol-baker` in N4.
**Every remaining area should expect it: after applying the map, grep the file
for the old prefix and expect ZERO.**

Verified: baked before and after — 102 removed, 102 added, table total unchanged;
every removed name is `__RTS_FN_NS_DOM_*`, every added name is `__rtsm_dom_*`,
and each mapped target was checked present, giving zero orphans. Repo-wide grep
for the old prefix across `*.rs` AND `*.ts`: zero. Full TS suite unchanged
(772/775 files, 2841/2853 tests).

### `input` + `render` — DONE (2026-07-31, 27 symbols)

Same mechanism and the same isolation test as `dom`: **zero consumers outside
the owning crate** for both `NS_INPUT` and `NS_RENDER`, and each lives in one
`abi.rs`. Verified 27 removed / 27 added, every removed name `__RTS_FN_NS_INPUT_*`
or `__RTS_FN_NS_RENDER_*`, every added name `__rtsm_input_*` or
`__rtsm_render_*`, zero orphans, zero old spellings left in `*.rs` or `*.ts`,
suite unchanged.

**The file this came from was SPLIT, not emptied.** `input_render_engine.tsv`
bundled three unrelated things; its remaining 14 rows are now `engine_core.tsv`,
and they are deliberately NOT in the same class as the rest of the map:

| rows | why they are harder |
|---|---|
| `__RTS_FN_NS_GC_*` (8), `__RTS_FN_RT_THIS_*` (2), `TRUTHY`, `TO_STRING_HANDLE`, `SEED`, `INIT` | Spread across **~20 files in 8 crates**, and — unlike every namespace area — several are named by STRING LITERALS in `rts-codegen-new` (`module_aot.rs` emits `__RTS_FN_RT_INIT` into the AOT `main` shim; `module_jit.rs`, `emit_marshal.rs`, `globalclass.rs` carry others). A wrong literal there compiles fine and **fails at RUNTIME or at LINK**, which is the one failure mode the bijection check cannot see. |

Order the remaining areas by that property, not by row count: a namespace whose
symbols are only ever named by the Rust item and one registration line is safe to
sweep mechanically; anything the codegen spells as a string needs the literal
swept in the same commit and the AOT path exercised, not just `rts run`.
