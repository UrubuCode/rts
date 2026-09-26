# `paint/` and `query/`: the two consumers of the fragment tree get their own modules

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** item 2 of the order fixed on 2026-09-25 (issue #2731): "move painting, `display` and stacking into `paint/`, and make the hit-test read the fragments". After BT-2 the layout returns one sequence of pieces keyed by box, with one rect per line fragment; what still lives beside the layout algorithms is everything that READS that output — the paint list's types and traversals, stacking, transforms, decoration — and the geometry queries (`rect_of`, `hit_test`, `getBoundingClientRect`). This plan moves the readers out of `layout/` and makes the hit-test a traversal of the fragments rather than a materialised table.

**Written 2026-09-25** from the tree at `728d14c34`, after reading every file named below. Item 3 of the order (regroup `layout/` by formatting context) comes AFTER this and is not this plan.

---

## The FORM, fixed first

1. **Three modules, one direction of dependency.** `layout/` PRODUCES (`DisplayList` of `Piece`s, `BoxRects`); `paint/` holds what the list IS and how it is painted (`DisplayItem`, `Piece`, the paint traversal, stacking, transforms, decoration emitters); `query/` holds what is ASKED of it after layout (`Geometry`, `rect_of`, `rect_cliente`, `hit_test`, `hit_test_clickable`, `rects_of_box`). `paint/` depends on nothing in `layout/` except the `BoxTree` and `Rect`; `query/` depends on `paint/` (it walks pieces) and on the tree; `layout/` depends on `paint/` (it emits items) and never on `query/`. A layout file that needs a geometry answer mid-layout (the out-of-flow pass reading the containing block's rect; `relativo`/`transformacao` shifting rects) reads `BoxRects` directly, which stays a layout-owned output — that is the one seam, and it is named.

2. **A move is a move.** Phase A changes no line of logic: files move, `mod` paths change, `pub(in crate::layout)` becomes `pub(crate)` where a moved item is read across the new boundary — and NOTHING else. The ruler is the paint-parity dump (176 PNGs byte-identical), `cargo test -p rts-dom --lib` with the same count, and the six WPT folders 0 lost / 0 gained. This is how `docs/ui/modularizacao-rts-dom.md` validated the last modularisation, and it is the reason a move must not be mixed with a fix.

3. **The hit-test reads the fragments (Phase B).** Today `hit_test` calls `geometry()`, which materialises `Geometry { rects: per node, hit_order: Vec<(NodeIdx, Rect)> }` by walking the pieces once and caching. Phase B makes `query::hit::hit_test(list, x, y)` walk the pieces in REVERSE paint order — `Piece::Child` descending into its fragment with its `dx`/`dy`, `Piece::Rect(box)` testing that box's own fragment rects (`BoxRects` per line fragment, BT-2c) — and answer the first hit's node. No table, no cache, no union: a link that wraps across two lines is hit by either fragment and not by the gap between them (the answer I4 exists to improve, `box-tree.md` §7). `hit_test_clickable` (pointer-events) is the same traversal with a predicate. `Geometry` stays for the DOM boundary (`rects` per node for `boundingRect`) and for the scroll regions; its `hit_order` field is deleted once nothing reads it.

4. **`itens.rs` stays in `layout/`.** `reserve_box_order`/`record_box_rect`/`add_line_fragment` WRITE the output; they are the producer's side of the seam. `box_fragments.rs` (`BoxRects`) stays in `layout/` for the same reason; `query/` reads it through `DisplayList`.

What this plan does NOT do: nested stacking contexts (SC), `getClientRects`, any change to `rts-egui`'s painter beyond import paths, the regroup of `layout/` (item 3).

---

## The state this plan starts from (read 2026-09-25)

| today | lines | what it holds | goes to |
|---|---|---|---|
| `layout/display.rs` | 539 | `Rect`, `Corners`, `DisplayItem` (13 variants), `ScrollRegion`, `DisplayList` (`tree`, `pieces`, `box_rects`, `grid_column_tracks`, `scroll_regions`, `canvas_background`, `content_height`, `geometry_cache`, `ancoras_estaticas`…), `Geometry`, `walk`/`materialized`/`materialize`/`geometry`/`geometry_now`/`rect_of`/`rect_of_box`/`rect_of_node`/`hit_test` | split: `paint/item.rs` (`DisplayItem`, `Corners`), `paint/list.rs` (`DisplayList`, `Rect`, `ScrollRegion`, the traversals), `query/geometry.rs` (`Geometry`, `geometry_now`, `rect_of_node`), `query/rect.rs` (`rect_of` → today's `rect_cliente.rs`), `query/hit.rs` |
| `layout/pieces.rs` | 238 | `Piece`, `walk`, `children`, `count_items`, `paints`, `collect`, `rect_in_children`, `rects_in_children`, `shift_from`, `flatten_from`, `legacy_tie_start` | `paint/pieces.rs`; `collect` (hit order) → `query/`; `shift_from`/`flatten_from` are written by layout (relative, transform) — they stay in `paint/pieces.rs` as the list's own editing primitives |
| `layout/empilhamento.rs` | 166 | `stacking_key`, `z_index_of`, `merge_before`/`merge_after` (negative z prepend) | `paint/stacking.rs` |
| `layout/transformacao.rs` | 365 | `Mat2d`, `TransformList`, `TransformOp`, `transform_box_rects`, `MAX_TRANSFORM_OPS` | `paint/transform.rs` (the matrix and list types); `transform_box_rects` writes `BoxRects` → stays in layout as `layout/transform_rects.rs` |
| `layout/pintura.rs` | 424 | `emit_scrollbar*`, `body_background`, `border_items`, `trapezios_dos_lados`, `decoration_code`, `cor_visivel`, `apply_opacity`, `deve_suprimir_fundo`, `is_text_input_tag`, `italico`, `tag_de` | `paint/decor.rs` (border/scrollbar/background emitters); the four style helpers (`decoration_code`, `cor_visivel`, `apply_opacity`, `italico`) → `paint/style.rs`; `is_text_input_tag`/`tag_de` are layout classification and stay in `layout/caixa.rs` |
| `layout/fundo_imagem.rs` | 155 | background-image emission | `paint/background_image.rs` |
| `layout/rect_cliente.rs` | — | `rect_cliente` (the DOM rect: boxes ∪ splitting blocks) | `query/rect.rs` |
| `dom/geometria.rs` | — | `bounding_component`, `hit_test_clickable` (pointer-events), `layout_cached` reuse | `hit_test_clickable` → `query/hit.rs`; the rest stays (it is the `Dom`'s API) |
| `layout/box_fragments.rs`, `layout/itens.rs` | 225, 84 | `BoxRects` (per-fragment rects), the writers | stay in `layout/` (the producer side) |

Consumers outside the crate: `rts-egui/src/frame/render/{pintura.rs,mod.rs,scroll.rs}` (`list.walk`, `DisplayItem::*`, `list.hit_test`, `Dom::hit_test_clickable`), `rts-egui/src/pixels.rs`, `rts-dom-bridge/src/{imagem.rs,nodes.rs}`, and `crates/rts-dom/examples/{claude-raster.rs,claude-paint-dump.rs,page_paint.rs,…}`. All of them reach these through `rts_dom::layout::{DisplayList, DisplayItem, Rect, …}` re-exports — **keep those re-exports in `layout.rs` during Phase A** so no consumer changes; retire them in Phase B once the new paths are in place, one crate at a time.

`crates/rts-dom/src/lib.rs` declares the top-level modules; `layout.rs` is at 453 lines and lists 82 submodules.

---

## Rulers

- **Phase A (moves):** paint-parity dump 176/176 byte-identical against the kept rasterizer (`target/claude-raster-base.exe`); `cargo test -p rts-dom --lib --no-fail-fast` same count (1 300 today) — a test that moves with its module keeps its name; `cargo check -p rts-egui -p rts-dom-bridge -p rts-ui`; the six WPT folders 0/0; the suite's lost list empty. A single differing pixel means the move changed a line of logic: find it, do not explain it away.
- **Phase B (hit-test):** the existing click tests in `crates/rts-dom/src/layout/tests/` (`clique_*`, `hit_*`, the `z-index` and `pointer-events` ones — grep `hit_test`), all green; plus new tests that pin the improvement: a link wrapped across two lines is hit on both lines and NOT in the gap after the first line's end; an `overflow:hidden` box does not hit its clipped child outside the clip (if the old table ignored clips, say so and decide with the coordinator whether Phase B honours `BeginClip` — it should, and that is a behaviour change to state). The DOM fixtures under `tests/*.test.ts` that click (`claude-dom-*click*`, `claude-hit-*`) run per file in the suite.

---

## Tasks

### Phase A — the moves (zero logic)

- [x] **A1 — `paint/`.** Create `crates/rts-dom/src/paint/mod.rs` and move: `layout/pieces.rs` → `paint/pieces.rs`; `layout/empilhamento.rs` → `paint/stacking.rs`; `layout/fundo_imagem.rs` → `paint/background_image.rs`; from `layout/transformacao.rs`, the types (`Mat2d`, `TransformList`, `TransformOp`, `MAX_TRANSFORM_OPS`, their parsing/resolving) → `paint/transform.rs`, leaving `transform_box_rects` + `shift_box_rects`-like writers in `layout/transform_rects.rs`; from `layout/pintura.rs`, the emitters → `paint/decor.rs` and the four colour/decoration helpers → `paint/style.rs`, leaving `is_text_input_tag`/`tag_de` in `layout/caixa.rs`. `layout.rs` keeps `pub use` re-exports of every moved public name so nothing outside changes. Ruler after A1.
- [x] **A2 — `DisplayItem`/`DisplayList` out of `display.rs`.** `paint/item.rs` (`DisplayItem`, `Corners`), `paint/list.rs` (`DisplayList`, `Rect`, `ScrollRegion`, `walk`, `materialized`, `materialize`, `total_items`, `push_item`, `for_dom`). `display.rs` is deleted when empty. Ruler after A2.
- [x] **A3 — `query/`.** `query/mod.rs`; `query/geometry.rs` (`Geometry`, `DisplayList::geometry`/`geometry_now`/`rect_of_node`/`rect_of_box` as `impl DisplayList` blocks in this module — Rust allows the impl to live beside its readers); `query/rect.rs` (today's `layout/rect_cliente.rs`, and `DisplayList::rect_of`); `query/hit.rs` (`DisplayList::hit_test`, `Dom::hit_test_clickable` moved from `dom/geometria.rs`). Ruler after A3.
- [x] **A4 — consumers on the new paths.** `rts-egui`, `rts-dom-bridge`, the examples and the tests import from `rts_dom::paint`/`rts_dom::query`; the `layout.rs` re-exports of moved names are deleted. `docs/ui/html-engine/box-tree.md` §6 ("What does not change": the public surface) and `docs/ui/egui-crate.md` name the new paths. Ruler after A4; PLAN.md §0 gains a row `PQ-A`.

### Phase B — the hit-test reads the fragments

- [x] **B1 — the traversal.** `query/hit.rs`: `hit_test` walks `pieces` in reverse, descends `Piece::Child` with its offsets, tests `Piece::Rect(box)` against `BoxRects`' per-fragment rects for that box (translated by the accumulated offset), answers `tree.node_of(box)` of the first hit; a `Piece::Rect` of a box with no node (anonymous) is skipped (the bridge promises nodes). `BeginClip`/`EndClip` on the way: a point outside an enclosing clip cannot hit what the clip encloses — implement it, and state it as the one behaviour change of Phase B, with a test. `hit_test_clickable` = the same walk with the pointer-events predicate.
- [x] **B2 — `Geometry.hit_order` dies.** `pieces::collect` stops building it; `Geometry` keeps `rects` (per node, for the DOM boundary) and `scroll_regions`; every reader of `hit_order` is gone (grep). The `geometry_cache` stays for `rects`.
- [x] **B3 — rulers**, plus the two new tests of "Rulers", plus PLAN.md §0 `PQ-B` row. If a click fixture in the suite moves, name the file and whether the new answer is the browser's (measure the click target in the Blink if in doubt: `scripts/css_fixtures_medir_edge.mjs` records `elementFromPoint`? — check; if not, `document.elementFromPoint` in a probe page).

---

## Constraints

- File ceiling 500 for every file in this crate; `display.rs` is at 539 and `paint/list.rs` must not inherit that — split as the table says.
- English identifiers and comments for everything NEW (module names, files); moved code keeps its names in Phase A (a rename is not a move — it comes after, as its own zero-change lot, the way `#2758` did).
- Comments say WHY; a moved file's header gains one line saying where it came from and when, nothing else changes in it.
- Agents: Phase A is ONE agent (moves conflict with everything; two agents moving files in one crate is the BT-3 split mistake again); no other layout lot runs in the same tree meanwhile. Phase B is one agent after A is merged.
