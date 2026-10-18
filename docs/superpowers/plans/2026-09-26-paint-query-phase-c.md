# Phase C of paint/query — the three seams point the right way

**Written 2026-09-26** from the tree at `0c19e036a`, after Phases A (#2766) and
B (#2769) of `2026-09-25-paint-and-query.md` and the regroup of `layout/`
(#2771). It executes the FORM 1 that plan fixed and did not finish: `layout/`
produces, `paint/` paints, `query/` answers, and the only thing that crosses
from layout into paint is what paint needs to paint.

## The three seams, as measured on 2026-09-26

1. **`paint/` reaches into `layout/` for a pure item utility and for a
   layout cache type.** `paint/pieces.rs:112,146` and `paint/list.rs:183`
   call `layout::fragment::items::translate_item`, a function that shifts a
   `DisplayItem` by `(dx, dy)` and touches nothing of layout. And
   `Piece::Child(ChildRef)` carries an `Rc<Fragment>` of which paint and
   query read exactly three fields — `pieces`, `rects`, `scroll_regions` —
   while the other ten (`box_id`, `tree`, `grid_column_tracks`,
   `direct_line`, `last_line`, `static_anchors`, `origin`, `size`,
   `margin_top`, `margin_bottom`) are relayout and restitch bookkeeping.
2. **`paint/list.rs` holds two things that are not paint's.**
   `DisplayList.box_rects: layout::BoxRects` (written by `layout/` at six
   sites and by `paint/stacking.rs:152,169`, read by `query/`) and
   `DisplayList.geometry_cache: RefCell<Option<Rc<query::Geometry>>>`, read
   and written by `query/geometry.rs` only — so `paint/list.rs` imports
   `query::Geometry`, the reverse of the plan's direction.
3. **`layout.rs:343,352` reads `geometry_now().rects` mid-layout**, in the
   out-of-flow pass, to know every node rect placed so far as a table of
   containing blocks. Seventy percent of that number is `box_rects.unions()`
   plus `tree.node_of`; the rest is the fold over reused `Piece::Child`
   subtrees that only `query/geometry.rs::add_children` implements today.
   `bounding_rect` (`layout.rs:415`) calling `rect_of` is NOT a seam: it is
   a post-layout public entry and reads query in the allowed direction.

## The FORM, fixed

**F1. `translate_item` lives in `paint/item.rs`.** Moved verbatim;
`layout/fragment/items.rs` keeps only the three producer writers
(`reserve_box_order`, `record_box_rect`, `add_line_fragment`). Any layout
caller of the shift (there is none today) would call `paint::item::translate_item`,
which is the allowed direction.

**F2. `Piece::Child` holds a paint-owned `Subtree`, and `Fragment` embeds it.**
`paint/pieces.rs` (or a new `paint/subtree.rs`) defines

```rust
pub struct Subtree {            // what a reused subtree IS to paint and query
    pub pieces: Vec<Piece>,
    pub rects: Vec<(BoxId, Rect)>,
    pub scroll_regions: Vec<ScrollRegion>,
}
```

and `ChildRef { subtree: Rc<Subtree>, dx, dy, box_id, height, margin_top,
margin_bottom }` — the fields paint, stacking and the stitch decision read
today, unchanged in meaning. `layout::fragment::types::Fragment` becomes
`Fragment { paint: Rc<Subtree>, box_id, tree, grid_column_tracks, direct_line,
last_line, static_anchors, origin, size, margin_top, margin_bottom }`, and
`emit_at` builds the `ChildRef` from `self.paint.clone()`. **The stitch
(`fragment/fragment.rs::stitch`) needs the full `Fragment` back from a
previous list's `Piece::Child`**: that is the one place layout reads a paint
piece to recover layout state, and it is resolved by the layout-owned cache
that already exists — `fragment_key`/`KeyBase` map to `Rc<Fragment>` in the
document's fragment cache, not through the piece. If the agent finds the
stitch reading `ChildRef.fragment` for something the cache cannot answer, it
stops and reports the field rather than widening `ChildRef`.

**F3. `box_rects` stays on `DisplayList` as the ONE seam, and only layout
writes it.** The plan named `BoxRects` as the seam and this phase keeps that:
`paint/list.rs` may hold `layout::BoxRects` (paint depends on layout for this
one type, stated in the field's doc). What changes: the two writes in
`paint/stacking.rs` (`target.box_rects.extend(...)` at :152 and :169) move to
the layout caller of `merge_before`/`merge_after` in `layout.rs`, so
`stacking` merges pieces and the caller merges rects. `paint/` reads
`box_rects` only in `stacking.rs:145` (`is_empty`), which stays.

**F4. `geometry_cache` leaves `DisplayList`; the memo lives where the
repeated reader is.** `query::Geometry` is built by `DisplayList::geometry_now()`
(unchanged) and memoised by the `Dom`: `dom/mod.rs::display_cache` already holds
`(key, Rc<DisplayList>)` for `layout_cached`; it gains an `OnceCell<Rc<Geometry>>`
beside the list, and `Dom::geometry_cached(&self, ctx) -> Rc<Geometry>` is the
accessor `dom/geometria.rs` (`bounding_component`, `bounding_components_many`),
`dom/scroll.rs` and `rts-egui`'s scroll-region reader use. `DisplayList::geometry()`
(the memoised form) is deleted; `DisplayList::rect_of` computes through
`geometry_now()` and its doc says so — it is the one-shot path
(`bounding_rect`, tests). `paint/list.rs` no longer names `query::`.
**The 13.7 ms-per-call class of 2026-09-16 is the ruler here**: the read loop
of `bounding_component` with no mutation between calls must still do ONE
layout and ONE geometry build, which the `Dom` memo guarantees and a test
pins (count `geometry_now` calls, or time 1 000 calls on the Wikipedia
fixture as `dom_metrics/scenarios.rs` does).

**F5. The out-of-flow pass reads a layout-owned table.**
`layout/fragment/known_rects.rs` (new, small): `known_rects(list: &DisplayList)
-> FastMap<NodeIdx, Rect>` = `box_rects.unions()` mapped through `tree.node_of`,
folded with the `rects` of every reused `Subtree` reached through `Piece::Child`
(the walk `query/geometry.rs::add_children` does today, moved here since
walking `Piece` is layout→paint, the allowed direction). `layout.rs:343,352`
call it; `query::geometry_now` calls it for its `rects` and keeps building
`scroll_regions` itself. One implementation of the fold, two callers.

## Rulers

Same four as every lot of this series, per file against `target/baseline.exe`
(= `main` at `0c19e036a`) and the kept reports, because none of F1–F5 may
change an answer:

1. 170/170 PNGs byte-identical (`claude-raster` over `tests/css/*.html`).
2. WPT six folders via `wpt_comparar.mjs` against `Documents/wpt-en2`: LOST
   none, GAINED none.
3. Corpus equal to `$TEMP/corpus-base.txt` modulo timings.
4. Suite equal to `$TEMP/suite-base.txt`: LOST none.
5. `cargo test -p rts-dom --lib`: 1 313 + the new memo test; every existing
   name unchanged.
6. **The read-loop cost**: the test of F4, plus `cargo run --release --example
   dom_metrics` numbers for `getBoundingClientRect` unchanged within noise
   (the coordinator runs this one; a `fast` or debug number is not a number).
7. The dependency direction, greppable: `grep -rn "crate::query\|crate::layout" crates/rts-dom/src/paint/`
   returns only the `BoxRects` field and its doc, and `use crate::boxes`,
   after this lot.

## Tasks

One agent, one worktree, in this order (each step compiles on its own):

- [x] **C1 — F1.** Move `translate_item`; fix the three callers; `items.rs` doc
  says what it keeps.
- [x] **C2 — F3.** `stacking` stops writing `box_rects`; the caller merges.
- [x] **C3 — F5.** `known_rects`; `layout.rs` and `geometry_now` call it.
- [x] **C4 — F4.** Memo to the `Dom`; `DisplayList::geometry()` deleted;
  `rts-egui`, `dom/`, `pseudo.rs`, `table/tests` callers repointed; the
  read-loop test.
- [ ] **C5 — F2.** *(stopped 2026-09-26: the stitch reads `origin`, `grid_column_tracks`, `last_line`, `static_anchors`, `tree` through `ChildRef.fragment`, and `remapped_to`/`stitch_total`/`static_anchor` too; nothing maps a `ChildRef` to its `FragmentKey`. Needs either those fields on `ChildRef` or a keyed side table — a decision, not a move.)* `Subtree` in paint; `Fragment` embeds it; `ChildRef` holds
  it; the stitch reads the fragment cache. This is the largest step and the
  last so the four before it are measurable if it has to stop.
- [ ] **C6** (coordinator): the rulers, the PLAN.md §0 `PQ-C` row, this file's
  boxes, and the seams paragraph in `box-tree.md` rewritten to say they are
  closed.

## Constraints

- No behaviour change. A gain in the WPT comparison is as suspicious as a loss.
- Files ≤ 500 lines; a step that would push one over splits by question first.
- One agent; it runs `cargo check`/`cargo test -p rts-dom --lib` and the
  four-crate check with its own `CARGO_TARGET_DIR`, never the release build,
  the rulers, or `git commit`.
- If a step needs a widening or a new `pub(crate)`, do it and list it.
- If F2 turns out to need the stitch to read a field the cache cannot
  answer, C5 stops with the field named, and C1–C4 are still delivered.
