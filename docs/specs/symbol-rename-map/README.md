# Symbol rename map — `__RTS_FN_*` → `__rtsm_` / `__rtsn_`

Input for phase **N5** of [`../../../RTS_ORGANIZATION.md`](../../../RTS_ORGANIZATION.md)
and step 6 of [`../no-mangle-drain.md`](../no-mangle-drain.md).

## What this is

626 legacy symbols mapped to the new convention. Each row is TSV:

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
* **`__rtsa_` / `Scope::Abi` is being deleted** (`RTS_ORGANIZATION.md` §N4). The
  50 rows currently spelled `__rtsa_*` in these files must be re-pointed at
  `__rtsn_*` before the rename runs. They were written before that decision.

Distribution: 576 `__rtsm_`, 50 `__rtsa_` (→ `__rtsn_`), 0 `__rtsn_`.
Verified: zero duplicates within the map, zero collisions against the baked table.

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
* **~150 symbols have no consumer whatsoever** (every buffer `ATOMICS_*`, 4
  `JSON_STRINGIFY_*`, ~104 of `collections` such as `SET_UNION` /
  `VEC_TO_SPLICED`, `THIS_GET`, `STRING_FREE`). Owner decision: rename now,
  audit for deletion separately (phase N7) — mixing deletion into a rename
  destroys the diagnosis if something breaks.
* `__RTS_FN_NS_GC_*` are NOT members of the `gc` namespace: `e.ns("gc")` exposes
  only `collect` / `live_count`, both already `__rtsn_*`.
