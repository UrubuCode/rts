# N7 — the consumerless symbols, measured

**Status: EXECUTED 2026-07-31. 166 symbols deleted**, baked table 2191 → 2025,
diff shows **166 removed and zero added**. Full TS suite unchanged against the
baseline (772/775 files, 2841/2853 tests, same failing set re-run individually).

Kept out of this pass, deliberately:

- the **13 macro-named `__rtsn_`/`__rtsadp_` machinery** entries at the bottom of
  this document. `__rtsn_stack_push`/`_pop`/`_depth` are the recursion-depth
  guard, and "dead" there means the CODEGEN STOPPED EMITTING THEM — a missing
  feature wearing a corpse's clothes. Deleting them would delete the diagnosis;
- the **10 `rts-egui/src/widgets.rs`** entries. `rts-egui` is under the MANDATORY
  egui-plan rule (`CLAUDE.md`): its frozen plan must be read in full first.

**One entry in this audit was WRONG and the deletion agent refused it**, which is
the check working: `__RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED` is listed below as
second-wave ("only caller is a first-wave corpse"). It is not — it is also called
by `__RTS_FN_GL_FUNCTION_REIFY` at `function/ops.rs:592`, which is live and on no
list. Deleting it would have been a hard compile error. Kept.

Measured 2026-07-31 against the baked table
(`crates/rts-symbol-baker/generated/symbol_table.rs`, 2191 rows at the time).

`RTS_ORGANIZATION.md` §5 N7 estimated "~150 symbols have no consumer at all".
Measured: **173**. The families it names are right; **its collections number is
not** — see the trap section, which is the part that would have broken the build.

## What counts as a consumer

A symbol is LIVE if any of these reference it, anywhere outside its own
definition:

1. a Rust call site or an `extern "C"` forward declaration in another file;
2. a Registry registration — `Member { symbol: "…" }`, `func(…, "SYM", …)`, or
   `SYM as *const u8`. **These have no Rust caller by design and are reachable
   from TS by their JS name.** A call-graph-only audit calls 370 of them dead;
3. a codegen reference in `crates/rts-codegen-new/` — `declare_function("…")`,
   `rtse::sym!`, an `abi_sig.rs` row, a bare string literal;
4. any `.ts` under `crates/*/src/`, `tests/`, `builtin/`;
5. the value model, `crates/rts-runtime/src/adapters/`.

Two heuristics that produce wrong answers, both hit during the audit:

- `unsafe extern "C" { fn X(); }` reads like a definition. Requiring `pub` on the
  definition line separates them — defs are `pub fn`, declarations are bare `fn`.
  Without that, 12 symbols were misfiled.
- A registration line lives in the same file as the definition, so "only its own
  file mentions it" marks **370 live registry members** dead.

## Counts

| | |
|---|---|
| baked table | 2191 (`__RTS_FN_*` 943 · `__rtsm_*` 926 · `__rtsadp_*` 259 · `__rtsn_*` 63) |
| **dead** | **173** = 160 `__RTS_FN_*` + 11 `__rtsn_*` + 2 `__rtsadp_*` |
| second wave (only caller is a first-wave corpse, same file) | 19, +18 `__rtsm_*` |
| registry-only, TS-reachable — **never delete** | 370 |
| `__rtsm_*` dead | **0** — every one is `#[rtse::abi(module=…/global=…, value="jsName")]`, i.e. a registry member by construction |

## ⚠️ The trap that would have broken the link

**The plan's "~104 of `collections`" is wrong.** There are 119
`__RTS_FN_NS_COLLECTIONS_*` in the table; **75 are dead, 44 are live.** Deleting
104 is roughly 29 link errors.

Live and called from ANOTHER crate — a hard link error on deletion:

| symbol | consumer |
|---|---|
| `VEC_LEN` / `VEC_GET` / `VEC_PUSH` / `VEC_NEW` | `rts-runtime/src/adapters/value/arraycb.rs:43,49,54,110` |
| `VEC_POP` / `VEC_SET` | `rts-runtime/src/adapters/value/arrayops.rs:172,241` |
| `VEC_SPLICE_AUTO` | `rts-runtime/src/adapters/value/arrayops.rs:691` |
| `VEC_TO_SPLICED_AUTO` | `rts-runtime/src/adapters/value/arrayops.rs:701` |
| `MAP_NEW` | `rts-primitives/src/function/ops.rs:18` |
| `MAP_GET_CHAIN` / `MAP_SET` / `MAP_HAS` / `MAP_DELETE` / `MAP_KEYS` / `MAP_GET_PROTO` / `MAP_DEFINE_PROPERTY` | `rts-primitives/src/proxy/ops.rs:17-24` (extern-C block) |
| `MAP_GET` / `MAP_GET_KH` | `rts-shared/src/collections/vec.rs:496` |
| `MAP_VALUES` | `rts-std/src/collector/string_pool.rs:53` |

Live only through the Registry, so no Rust caller exists (12): `MAP_FREE`,
`MAP_LEN`, `MAP_CLEAR`, `OBJ_HAS`, `MAP_SET_KH`, `OBJ_SET`, `OBJ_GET`,
`MAP_CLONE`, `MAP_KEY_AT` (`collections/map.rs`); `VEC_FREE`, `VEC_CLEAR`,
`VEC_JOIN` (`collections/vec.rs`). Registered through `append_engine_members` →
`ns::collections::register` at `rts-codegen-new/src/front/run/registry_build.rs:37`.

**Never delete by prefix glob.** `VEC_TO_SPLICED` is dead but
`VEC_TO_SPLICED_AUTO` is live; `VEC_SPLICE_REMOVE`/`_INSERT` are dead but
`VEC_SPLICE_AUTO` is live. `grep VEC_TO_SPLICED` returns 7 hits for a symbol with
2 — the substring matches its live siblings. Match the whole name.

Everything else the plan named checks out: `ATOMICS_*` 4/4 dead; `JSON_STRINGIFY_*`
exactly 4 of 6 (`__RTS_FN_NS_JSON_STRINGIFY` and `_STRINGIFY_PRETTY` are live);
`THIS_GET`; `STRING_FREE`; `SET_UNION`.

## The dead list, by defining file

### `crates/rts-shared/src/collections/vec.rs` — 46
`__RTS_FN_NS_COLLECTIONS_` unless shown: `:115` VEC_AT_AUTO · `:279` VEC_EXTEND_FROM ·
`:302` VEC_FILL_TA_ARG · `:332` VEC_EXTEND_FROM_BUFFER · `:358` VEC_MIN · `:365` VEC_MAX ·
`:377` VEC_SET_LENGTH · `:403` INDEX_DELETE_AUTO · `:450` INDEX_GET_AUTO · `:516` CONCAT_AUTO ·
`:534` SLICE_AUTO · `:550` INCLUDES_AUTO · `:562` INDEX_OF_AUTO · `:578` LAST_INDEX_OF_AUTO ·
`:595` VEC_HAS_INDEX · `:608` VEC_SET_FROM · `:727` VEC_INDEX_OF_FROM · `:759` VEC_LAST_INDEX_OF_FROM ·
`:797` VEC_INCLUDES_FROM · `:817` VEC_REVERSE · `:825` VEC_SHIFT · `:842` VEC_UNSHIFT ·
`:941` VEC_CONCAT_VARIADIC · `:983` VEC_TAKE · `:995` VEC_UNSHIFT_VARIADIC · `:1008` VEC_FILL ·
`:1039` VEC_FLAT · `:1079` VEC_FLAT_DEPTH · `:1090` VEC_SPLICE_REMOVE · `:1111` VEC_SPLICE_INSERT ·
`:1204` `__RTS_FN_GL_ARRAY_FROM_VEC` · `:1266` VEC_FIND_LAST · `:1281` VEC_FIND_LAST_INDEX ·
`:1299` VEC_REDUCE_RIGHT · `:1325` VEC_REDUCE_RIGHT_NO_INIT · `:1349` VEC_FLAT_MAP ·
`:1374` VEC_COPY_WITHIN · `:1415` VEC_SORT · `:1473` VEC_VALUES · `:1510` VEC_KEYS ·
`:1535` VEC_TO_SORTED · `:1578` VEC_TO_REVERSED · `:1586` VEC_TO_SPLICED ·
`:1605` VEC_TO_SPLICED_INSERT · `:1632` VEC_WITH · `:1652` VEC_ENTRIES

### `crates/rts-shared/src/collections/map.rs` — 32
`:757` `__RTS_FN_RT_GLOBAL_THIS_MAP` · `:766` MAP_GET_DIRECT · `:832` REGISTER_CLASS_METHOD ·
`:860` MAP_FOR_EACH · `:907` SET_FOR_EACH · `:1064` MAP_DELETE_AUTO · `:1091` MAP_ENTRIES ·
`:1189` PREVENT_EXTENSIONS · `:1199` IS_EXTENSIBLE · `:1213` MAP_ASSIGN ·
`:1228` OBJECT_OWN_PROPERTY_NAMES · `:1387` FOR_IN_KEYS · `:1479` MARK_AS_MAP ·
`:1539` MAP_FREEZE · `:1546` MAP_SEAL · `:1552` MAP_IS_FROZEN · `:1557` MAP_IS_SEALED ·
`:1787` SET_UNION · `:1810` SET_INTERSECTION · `:1839` SET_DIFFERENCE ·
`:1868` SET_SYMMETRIC_DIFFERENCE · `:1930` SET_IS_SUPERSET · `:1936` SET_IS_DISJOINT ·
`:1958` OBJECT_GROUP_BY · `:1965` MAP_GROUP_BY · `:2018` MAP_FROM_ENTRIES · `:2092` SET_ADD ·
`:2159` HAS_AUTO · `:2176` DELETE_AUTO · `:2194` MAP_GET_AUTO_H ·
`:2213` `__RTS_FN_RT_FOR_OF_NORMALIZE` · `:2234` SET_FROM_VEC

### `crates/rts-std/src/globals/fetch/instance.rs` — 15
`:775` FETCH_RESPONSE_STATUS · `:786` _OK · `:797` _STATUS_TEXT · `:822` _URL · `:830` _TEXT ·
`:848` _JSON · `:857` _ARRAY_BUFFER · `:899` _NEW · `:926` _HEADERS · `:988` _THEN · `:1007` _FREE ·
`:935` REQUEST_NEW · `:955` REQUEST_METHOD · `:963` REQUEST_URL · `:972` REQUEST_TEXT

### `crates/rts-primitives/src/error/instance.rs` — 15
`:74` ERROR_NEW · `:92` REF_ERROR_NEW · `:98` SYNTAX_ERROR_NEW · `:105` ERROR_CAUSE ·
`:113` URI_ERROR_NEW · `:119` EVAL_ERROR_NEW · `:129` AGGREGATE_ERROR_NEW ·
`:136` AGGREGATE_ERROR_ERRORS · `:149` IS_ERROR · `:170` IS_ERROR_NAMED · `:285` ERROR_MESSAGE ·
`:290` ERROR_NAME · `:299` ERROR_STACK · `:319` ERROR_TO_STRING · `:334` ERROR_CAPTURE_STACK_TRACE

> `__RTS_FN_GL_RANGE_ERROR_NEW` in the same file is **live** — the recursion-depth
> guard (`rts-natives/src/collector/stack.rs`) calls it to build the
> `RangeError` for "Maximum call stack size exceeded".

### `crates/rts-egui/src/widgets.rs` — 10
`:158` EGUI_DEFINE_BLOCK · `:187` _DEFINE_STYLE · `:210` _QUERY_SELECTOR · `:222` _SET_TEXT ·
`:234` _SET_ATTR · `:255` _CREATE_ELEMENT · `:267` _APPEND_CHILD · `:282` _REMOVE_NODE ·
`:296` _DEFINE_INLINE · `:311` _DOM_DUMP

> Hand-written `pub extern "C" fn`, no `#[rtse::abi]`. The other 33 EGUI symbols
> in the table ARE string-registered — do not sweep the file.
> **`rts-egui` is under the MANDATORY egui-plan rule** (`CLAUDE.md`): read
> `docs/specs/html-engine/` in full before touching it.

### `crates/rts-primitives/src/function/ops.rs` — 9
`:615` FUNCTION_REIFY_BOUND · `:704` FUNCTION_REIFY_CAPTURED · `:1037` FUNCTION_BIND ·
`:1205` RT_REGISTER_FN_DEFAULTS · `:1399` RT_INSTANCEOF_PROTO · `:1440` RT_INVOKE_AUTO_TYPED ·
`:1462` RT_INVOKE_AUTO_AS_F64 · `:1750` FUNCTION_PROTOTYPE_SET · `:1799` FUNCTION_TO_STRING

### `crates/rts-shared/src/buffer/mod.rs` — 7
`:933` ATOMICS_RMW · `:959` ATOMICS_CAS · `:976` ATOMICS_LOAD · `:987` ATOMICS_STORE ·
`:1002` TA_SET_FROM · `:1023` TA_LENGTH · `:1036` BUFFER_DETACH

### smaller groups
- `rts-shared/src/json/mod.rs` — 4: `:586` STRINGIFY_REPLACER_FN · `:652` STRINGIFY_KEYS · `:771` STRINGIFY_TYPED · `:928` STRINGIFY_PRETTY_STR
- `rts-std/src/globals/console/rt.rs` — 4: `:27` RT_CONSOLE_SET_OVERRIDE · `:49` _GET_OVERRIDE · `:59` _OVERRIDE_IS_VARIADIC · `:134` GL_CONSOLE_WRITE_AUTO
- `rts-primitives/src/proxy/ops.rs` — 4: `:292` REFLECT_CONSTRUCT · `:327` REFLECT_SET_PROTOTYPE_OF · `:454` REFLECT_DEFINE_PROPERTY_PROXY · `:466` REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY
- `rts-primitives/src/reflect/ops.rs` — 3: `:54` REFLECT_GET_OWN_PROPERTY_DESCRIPTOR · `:110` OBJECT_GET_OWN_PROPERTY_DESCRIPTORS · `:201` REFLECT_DEFINE_PROPERTY
- `rts-std/src/promise/mod.rs` — 2: `:947` PROMISE_AWAIT_VALUE · `:965` GL_ARRAY_FROM_ASYNC
- `rts-primitives/src/regexp/mod.rs` — 2: `:494` REGEXP_INDICES_GROUPS · `:504` REGEXP_LAST_INDEX_SET
- `rts-primitives/src/function/props.rs` — 2: `:40` RT_FUNCTION_SET_PROP · `:81` RT_FUNCTION_TO_STRING_DYN
- `rts-natives/src/heap/string_pool/alloc.rs` — 2: `:37` `__RTS_FN_NS_GC_STRING_FREE` · `:98` GC_STRING_FROM_I64
- `rts-natives/src/heap/this_slot.rs:35` — `__RTS_FN_RT_THIS_GET`
- `rts-primitives/src/symbol/mod.rs:208` — RT_TO_PRIMITIVE
- `rts-std/src/crypto/mod.rs:238` — NS_CRYPTO_SHA256_DIGEST

### macro-named (`value = "…"`, so the symbol has no textual definition) — 13
`rts-natives/src/collector/stack.rs:42/59/65` → `__rtsn_stack_push` / `_pop` / `_depth` ·
`cycle.rs:212` → `__rtsn_collect_debt` · `error.rs:171` → `__rtsn_report_uncaught` ·
`rts-std/src/collector/string_pool.rs:37/127/179` → `__rtsn_spread_into_vec` /
`__rtsn_object_to_string` / `__rtsn_inspect` ·
`rts-std/src/collector/generator.rs:927/1186` → `__rtsn_gen_sm_is` / `__rtsn_symbol_iterator_of` ·
`rts-runtime/src/adapters/value/genops_arith.rs:276` → `__rtsadp_not` (its own doc says
"NOT wired from the HIR") · `.../inspect.rs:94` → `__rtsadp_inspect_object` ·
`rts-natives/src/collector/error.rs:92` → `__rtsn_error_get_stack`

> `__rtsn_error_get_stack` needs a **second** edit: its only mention anywhere is a
> declaration with no call site, `rts-std/src/gc_surface.rs:28`. Delete both lines
> together. `__rtsn_error_get`, `__rtsn_error_clear`, `__rtsn_async_sm_resume` and
> `__rtsadp_fn_invoke` in that same block ARE called — leave them.

> `__rtsn_stack_push`/`_pop` look alarming to delete: they are the recursion-depth
> guard the codegen was meant to emit at every non-tail user function. Being dead
> means **the codegen no longer emits them**, not that the guard is unnecessary.
> Confirm which before deleting — this one is a missing feature wearing a corpse's
> clothes, and `stack.rs`'s `RangeError` path goes with it.

## Second wave — dies only once wave 1 lands (19)

Called exclusively by a wave-1 corpse in the same file, so they must go in the
SAME commit or they become `dead_code` warnings (which this repo treats as errors).

`collections/vec.rs`: VEC_CONCAT `:884`←`:520` · VEC_CONCAT_APPEND `:895`←`:521` ·
VEC_SLICE `:860`←`:538` · VEC_INCLUDES `:786`←`:554` · VEC_INDEX_OF `:713`←`:566` ·
VEC_LAST_INDEX_OF `:747`←`:582`
`collections/map.rs`: MAP_GET_AUTO `:190`←`:2201` · MAP_ENTRIES_INSERTION `:1137`←`:2218` ·
OBJECT_KEYS_AUTO `:1295`←`:1392` · MARK_AS_SET `:1484`←`:2241` ·
SET_IS_SUBSET `:1909`←`:1931` · SET_OR_MAP_HAS `:2112`←`:2166` ·
SET_OR_MAP_DELETE `:2130`←`:2183`
also `function/ops.rs:645` FUNCTION_REIFY_BOUND_TYPED · `function/ops.rs:1336`
FUNCTION_PROTOTYPE_GET · `function/props.rs:61` RT_FUNCTION_GET_PROP ·
`heap/string_pool/alloc.rs:43` GC_HANDLE_LEN · `rts-node/src/dgram/pump.rs:48` ·
`rts-node/src/net/tcp/pump.rs:45`. (+18 `__rtsm_*` of the same shape.)

## Ordering — N7 must run BEFORE N5

**71 of these 173 dead symbols have rows in `docs/specs/symbol-rename-map/*.tsv`**
(626 rows): the rename campaign plans to re-spell symbols this campaign plans to
delete — `__RTS_FN_GL_ERROR_NEW → __rtsm_global_Error_new`
(`primitives_std.tsv:25`), all 4 `ATOMICS_*`, all 4 dead `JSON_STRINGIFY_*`, all
11 fetch `RESPONSE_*`, `__RTS_FN_NS_GC_STRING_FREE`.

Run N7 first and drop those 71 rows, or N5's "bijection with zero orphans" proof
fails.

There is a semantic decision hiding in that overlap, and it is not a bookkeeping
detail: those rows would turn a currently-UNREACHABLE symbol into an
`__rtsm_global_*` **registry member** — N5 as written would *resurrect* the
`Error` / `Reflect` / fetch families as TS surface rather than delete them.
Decide which is intended per family before deleting anything.

## Verification protocol for the deletion

Same as `RTS_ORGANIZATION.md` §6, with one addition:

1. Bake before and after; diff the NAME SETS. A deletion must show **N removed,
   zero added**, and N must equal the number of symbols in the commit.
2. Match **whole symbol names**, never a prefix (see the trap above).
3. `cargo check --workspace` catches a missing Rust caller. It does **not** catch
   a missing Registry string, a `declare_function("…")` literal, or a `.ts`
   consumer — those fail at LINK or at RUNTIME. Grep each name across `*.rs` AND
   `*.ts` before removing it.
4. Delete wave 1 and its wave-2 dependents in one commit (`dead_code` is an error
   here).
5. Run the TS suite; the failing SET must equal the measured baseline.
