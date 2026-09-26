# Vertical writing modes — rotate in, lay out, rotate out

**Written 2026-09-26** from the tree at `696c80cc0`, after the css-position
triage (`2026-09-25-triagem-css-position.md`, item 1: ~25 files, the single
largest cause) and a survey of what the engine already has. It is the plan an
agent executes; the rules are `crates/rts-dom/PLAN.md` §1–§2 and
`docs/ui/html-engine/box-tree.md` §7.

## What is true today, measured

- `writing-mode` is parsed (`style/parse/fluxo.rs:104`), stored inherited
  (`style/props/tabela.rs:461`), and read by exactly three things that lay out:
  `layout/flex/axes.rs` (which physical axis is flex's main axis, composed
  from `style::text::{eixo_x_forward, eixo_y_forward}` — the ONE place with a
  working axis swap, proven on `flexbox-writing-mode-*`), `block/block.rs:555`
  (only to feed that flex dispatch) and `block/rtl.rs:75` (a gate). Block
  flow, inline flow, floats, positioning, tables and grid treat every box as
  `horizontal-tb`.
- Two logical pieces exist without a consumer: `style/values/axes.rs`
  (`AxisMap { wm, dir }`, 437 lines, 15 unit tests, correct) and
  `style/values/containing_block.rs` (`ContainingBlock`, built to be fed by
  `AxisMap`; its one construction site, `positioned/containing_block.rs:73`,
  calls `horizontal_tb(...)`). `style/logical.rs::to_physical` maps the
  `*-inline-*` logical properties honouring vertical modes, but pins
  `block-start/end` to top/bottom by design ("the engine never lays out
  vertically").
- Everything below layout is physical: `Rect {x, y, w, h}`, `LayoutCtx
  {viewport_w, viewport_h}`, `avail_w/avail_h` (31 sites in `block.rs` +
  `vertical_flow.rs`), `content_w/content_h` (69 sites). `DisplayItem::Text`
  has no orientation; `paint/transform.rs` has `Mat2d::rotate_deg` for CSS
  transforms; the raster paints glyph bitmaps upright; egui cannot draw
  rotated text.
- WPT files whose source uses a vertical mode: css-position 42 (0 of the 6
  measured pass), css-flexbox 196 (47 of 86 measured pass — flex's swap
  works), css-grid 418, css-sizing 74. The dedicated folder
  `css/css-writing-modes` (1 085 reftests) is NOT in the local corpus yet.
- The cheapest sub-cases the numbers show: (a) a vertical block holding only
  replaced or empty boxes — `top-layer-box-uses-icb-{slr,srl,vlr,vrl}-*`,
  8 files, no glyphs at all; (b) a vertical block with Ahem-only text — 29 of
  the 42 css-position files (`static-position/vlr-*`, `multicol/*`): Ahem
  glyphs are squares, so "rotate the text" is "stack squares", which needs no
  rotated glyph drawing.

## The FORM, fixed first

**F1. A vertical box is laid out in a ROTATED FRAME and its output rotated
back — the hundred physical call sites do not change.** When a block
container's `writing-mode` is vertical (and its parent's is not — the
boundary), layout does three things, in `layout/block/rotated.rs` (new):

1. **Rotate in.** The subtree's computed styles are viewed through the frame:
   physical sides are re-labelled to what the horizontal algorithm expects
   as logical. For `vertical-rl`: inline axis = physical y (top → bottom),
   block axis = physical x (right → left). So the algorithm's `width` is the
   author's `height`, its `margin-left` is the author's `margin-top`, its
   `margin-top` is the author's `margin-right` (block-start = right), and so
   on for padding, border, insets, min/max, `float: left/right` (→ inline-
   start/end), `text-align` (already logical). `AxisMap` is the mapping and
   `style/logical.rs` gains the block-axis half it lacks — that is the ONE
   place the six side properties are permuted, as a pure function
   `ComputedStyle -> ComputedStyle` applied once per node of the subtree
   (a cached clone; vertical subtrees are rare). Percentages then resolve
   correctly by construction: the algorithm's `avail_w` IS the inline size.
2. **Lay out** the subtree with the existing horizontal machinery, where
   the algorithm's `avail_w` is the box's INLINE size — the container's
   physical height (for the root, the ICB height) — and its `avail_h` is
   the box's BLOCK size — the container's physical width when definite,
   else `None`.
3. **Rotate out.** Every `Rect` the subtree recorded — box rects, line
   fragments, display items, static anchors, scroll regions — is mapped
   `(x, y, w, h) → physical` by one function of the frame: `vertical-rl`:
   `px = right − (y + h)`, `py = top + x`, `pw = h`, `ph = w`; `vertical-lr`:
   `px = left + y`, `py = top + x`. `Fragment.rects`, `BoxRects` and the
   pieces are transformed in place before the parent reads them, through
   the same `translate_item`-style walk `transform_box_rects` already does
   for CSS transforms — reused, not rewritten.

Text is the one item that cannot be transposed as a rectangle: a
`DisplayItem::Text` in a rotated subtree gains `orientation:
Orientation::{Horizontal, SidewaysRl, SidewaysLr, Upright}` (new field; the
rotation decides sideways for Latin in `vertical-*`, upright only under
`text-orientation: upright` / CJK, which this plan does not do). Its `(x, y)`
is the rotated-out line start. The raster (T2) rotates each glyph bitmap 90°
for sideways runs; Ahem stays squares, exact. egui paints sideways runs as
squares for Ahem and upright at the rotated position otherwise (stated cut:
egui has no rotated text; `paint/transform.rs::rotate_deg` on the item is the
follow-up).

**F2. Orthogonal flows (Writing Modes §7.3) are the same mechanism one level
down.** A child whose mode differs from its parent's is laid out as an
atomic box in its OWN frame: its available inline size is the containing
block's block size if definite, else the ICB's (the spec's rule), then its
result is rotated out into the parent's frame. Nested rotations compose;
a horizontal box inside a vertical one is the common case in the tests.

**F3. Flex and grid keep their own axis logic.** `flex/axes.rs` already
answers which physical axis is main; a flex container in a vertical mode is
laid out by the rotated frame like any block, and its `main_on_y_axis` is
asked in the ROTATED frame (where the mode reads as horizontal), which is
what makes the two agree. If a flex test that passes today is lost by that,
the frame is wrong and the lot stops there.

**F4. `getBoundingClientRect`, hit-testing and painting read physical rects
only**, which is what rotate-out guarantees; nothing in `query/`, `paint/`
or `rts-egui` learns about frames except the text orientation.

## Rulers

1. `css/css-writing-modes` added to the corpus sparse-checkout (1 085
   files) and swept BEFORE the first edit as the base — the primary ruler.
2. The eight folders, per file, against `Documents/wpt-base8`: LOST none;
   the css-position vertical families (`static-position/{vlr,vrl}-*`,
   `top-layer-box-uses-icb-{slr,srl,vlr,vrl}-*`, `multicol/*`) are the
   expected gains of WM-1.
3. Corpus 168/170 and suite unchanged (no fixture there is vertical).
4. Unit tests with numbers derived by hand: a `vertical-rl` body of height
   300 holding two 50 px-thick blocks puts the first at `x = right − 50` and
   the second left of it; a `vertical-lr` puts them from the left; Ahem
   text "XX" at 20 px in `vertical-rl` occupies a 20 × 40 column; margins
   `top/right` map to inline-start/block-start.
5. `dom_metrics` release unchanged on `pagina.combinada.html` (no vertical
   box → the rotation is never entered; the cost must be zero when absent).

## Tasks

- [ ] **WM-0 — the base.** `git -C Documents/wpt-corpus sparse-checkout add
  css/css-writing-modes`; base sweep with the current raster; record the
  count in PLAN.md §0 (coordinator).
- [ ] **WM-1 — the rotated frame for block flow, Ahem and replaced content
  only** (one agent, Opus): `logical.rs` block-axis half; `block/rotated.rs`
  (rotate in, lay out, rotate out); `Text.orientation` with the raster
  rotating bitmaps and squares exact; the sub-cases (a) and (b) above are the
  named targets. No orthogonal flows yet (a differing child mode is laid out
  as if the parent's — stated).
- [ ] **WM-2 — orthogonal flows** (§7.3), nested composition.
- [ ] **WM-3 — floats, positioned boxes and static positions in a rotated
  frame** (the css-position `static-position/vlr-*` families need the
  anchors rotated out; `containing_block.rs` finally built from `AxisMap`).
- [ ] **WM-4 — flex and grid containers in vertical modes** (F3, measured on
  the 196 flexbox and 418 grid files).
- [ ] **WM-5 — egui paints sideways text** through a rotation on the item.

## Constraints

- No physical call site of `block.rs`/`vertical_flow.rs` is rewritten to
  logical in this plan; the frame is the whole design, and a lot that starts
  threading `inline_size` through those hundred sites has left the plan.
- `text-orientation`, `text-combine-upright`, `sideways-*` modes and bidi
  in vertical lines are out.
- One agent per task; the coordinator builds release and runs rulers 1–5.
