# `rts-dom` — retained DOM as an independent crate (headless)

> Status: **implemented** (2026-06-25). Extracted from `rts-egui`. Allows reusing the
> DOM from TS without opening a window, and keeps `rts-egui` as a mere consumer of the tree.

## Motivation

The DOM (HTML parser + arena tree + versioned `NodeId` + query/mutation) was born
inside `rts-egui`, but **has nothing UI about it** — it is manipulation of a data
tree. Living in the UI crate, it was tied to the window: the `Dom` lived in the `UiCtx`
and the **entire** API (`querySelector`/`setText`/…) required a window handle. This
prevented two legitimate reuses:

1. **Headless TS** — parse/query/mutate HTML in memory without rendering.
2. **Other backends** — any renderer (not just egui) being able to read the same tree.

## Decision

New crate **`crates/rts-dom`**, depending ONLY on `rts-engine` (like `rts-egui`):

- **`dom.rs`** — `Dom` (arena `Vec<Node>`), versioned `NodeId { gen, idx }`
  (invariant 2), internal `NodeIdx`, O(1) query by `#id`/`.class` + pre-order by
  tag, mutation (`set_text`/`set_attr`/`create_element`/`append_child`/`remove_node`).
- **`html.rs`** — minimal HTML tokenizer + `decode_entities` (named + numeric).
- **`abi.rs`** — HEADLESS `rts:dom` namespace: `thread_local` store of standalone
  `Dom`s (its own `u64` handle — the engine does NOT know the DOM, doctrine), and the
  members `parseHtml`/`createDocument`/`free`/`querySelector`/`setText`/`setAttr`/
  `createElement`/`appendChild`/`removeNode`/`rootId`/`dump`.

### Why its own `thread_local` store (and not the engine's `Entry`)

The `HandleTable`'s `Entry` (rts-engine) is the CLOSED list of variants the engine
knows. Adding `Entry::Dom` would make the **engine name the DOM** — it violates the
PRIMORDIAL doctrine (the DOM is non-primordial; the engine only knows primitives). So
`rts-dom` keeps its own `thread_local HashMap<u64, Dom>`, exactly like
`rts-egui` keeps the `UiCtx` in a thread_local outside the `Entry`.

### ABI conventions (all followed)

- Symbols `__RTS_FN_NS_DOM_*` (convention `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>`).
- The DOM handle crosses as `u64`; **`NodeId` crosses VERSIONED in an `i64`**
  (`to_abi`/`from_abi`, `(gen<<32)|idx`).
- "None" sentinel = **`-1`** (invariant 3 — `u64::MAX` is not exact as a
  `number`). TS rule: extract the return into a const before comparing.
- No polymorphic value at the boundary; strings come in as `StrPtr`.

## Registration (data-driven, the engine does not name "dom")

Same pattern as `egui`:

- `rts-runtime`: dep + `pub use rts_dom as dom;` (`namespaces/mod.rs`).
- `rts-codegen-new/registry_build.rs`: one row in the `REGISTER` table
  (`Register { label: "dom", run: ns::dom::register, … }`). The front NEVER writes
  `"dom"` in control flow; it is data in the table.
- The real `fn_ptr`s in the `Member`s are harvested and installed in the JIT by
  `adapter_symbols`, like any namespace.

## How `rts-egui` consumes it

`rts-egui` depends on `rts-dom` and does `pub(crate) use rts_dom as dom;` — so
`crate::dom::Dom`/`NodeId`/`parse_html_to_dom` keep resolving in `ctx.rs`/
`frame/render.rs`/`widgets.rs` WITHOUT changing each call site. The `UiCtx` holds an
`rts_dom::Dom`; the render reads the tree directly (not through the ABI). The `egui.*` DOM
fns (with a window handle) remain as an ergonomic shortcut operating on the `UiCtx`'s
`Dom` — a path parallel to the headless `rts:dom` ABI.

```
crates/rts-dom/        pure DOM + headless rts:dom ABI   ← TS reuse without a window
   ↑ (consumes the Dom type)
crates/rts-egui/       only the RENDER (frame/render.rs reads rts_dom::Dom)
```

## Validation

- `rts-dom`: 27 tests (tree/parser/entities + 3 for the headless ABI).
- `rts-egui`: 12 tests (style/box) — still renders consuming `rts-dom`.
- Headless E2E: `examples/claude-dom-headless.ts` (parse/query/mutation/create without
  a window).
- Render E2E: `examples/claude-egui-box-complexo.ts` (egui draws the tree).

## Future (out of this scope)

- Ergonomic `Document`/`Element` facade in TS over `rts:dom` (planned in F3
  of the web-engine roadmap, invariant 5 — `.ts` lib via prelude).
- `getText(node) → Handle` (text read) — prerequisite for the facade.
```
