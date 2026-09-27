# Triage of the css-grid WPT reftest failures — 2026-09-26

Measured on `main` = `1ff2031bd` with `target/raster-base.exe`, from the sweep in
`Documents/wpt-base2/css-grid/relatorio.json` (1 627 reftests: 337 pass, 34
pass-empty, 1 231 fail, 25 partial). Method as in
`2026-09-25-triagem-css-position.md`: group by subfolder and pixel-delta, read
one test per group, render a few. Five pairs were rendered; ~25 sources read.

## The denominator first

**881 of the 1 627 (54 %) are `display: grid-lanes`** — the CSS Grid Level 3
draft (the renamed masonry proposal), with `flow-tolerance` and its own
`abspos/`, `alignment/`, `item-placement/`, `subgrid/` and `track-sizing/`
subtrees. No token of it exists under `layout/grid/`, and its 124 "passes" are
coincidences of simple layouts. Blink does not ship it either. Per the house
rule (do not measure in Blink what Blink does not have), **the grid ruler is
the other 746 files**: 213 pass, 26 empty, 483 fail, 24 partial.

| subfolder | pass | empty | fail | partial | total |
|---|---:|---:|---:|---:|---:|
| grid-lanes (EXCLUDED) | 124 | 8 | 748 | 1 | 881 |
| alignment | 59 | 0 | 108 | 0 | 167 |
| grid-items | 38 | 3 | 115 | 6 | 162 |
| abspos | 13 | 0 | 126 | 0 | 139 |
| subgrid | 33 | 1 | 64 | 0 | 98 |
| (root) | 22 | 3 | 24 | 6 | 55 |
| grid-model | 10 | 19 | 9 | 11 | 49 |
| layout-algorithm | 15 | 0 | 15 | 0 | 30 |
| grid-definition | 14 | 0 | 11 | 0 | 25 |
| placement | 9 | 0 | 7 | 0 | 16 |
| implicit-grids | 0 | 0 | 2 | 1 | 3 |
| animation | 0 | 0 | 2 | 0 | 2 |

## Causes, ranked by files per cause

| # | feature | files | MEASURED / HYPOTHESIS | where |
|---|---|---:|---|---|
| 1 | **abspos static position ignores the grid container's `align-items`/`justify-items`** | ≥ 62 (`*-staticpos-*`) plus shares of the 10 000/5 000/2 500 px groups, ~90 | MEASURED: `grid-abspos-staticpos-align-items-center-large-border-padding` paints the green box at y 105–204, the ref at 305–404 — exactly the missing vertical centring, Δ = 10 000 px | `layout/positioned/static_position.rs:~137` says "grid still follows the block rule" and never reads `align_items`/`justify_items` |
| 2 | **`aspect-ratio` absent from grid item and track sizing** | ~31 (+ ~6 in `grid-definition`/`layout-algorithm`) | source of `grid-items/aspect-ratio-001`: `aspect-ratio: 1/1; height: 100 %` in a 100 px row must give a 100×100 square; `<meta name=assert>` names the cause | `aspect_ratio` exists only in `block/block.rs` and `positioned/positioned.rs`; zero occurrences under `layout/grid/` |
| 3 | **subgrid unimplemented** | 64 | the keyword is ignored and the outer tracks are reused; the 33 passes are the cases where that coincides | none — `grid/subgrid.rs` to write, hooked into `lines.rs`/`tracks.rs` |
| 4 | **abspos containing block ignores the container's `max-width`/`max-height` clamp** | ≥ 1 confirmed, family `positioned-grid-items-0NN` has 39 failing | MEASURED: `positioned-grid-items-018` paints NOTHING against a 100 px green ref: `width:100%`/`height:100%` of the abspos child collapse | `positioned/containing_block.rs` + `static_position.rs` |
| 5 | named lines, multi-value `repeat()`, `fr` with `%` | ~15–20, small deltas (1–15 %) | HYPOTHESIS: partial code with edge bugs, not absence | `grid/tracks.rs` (196), `grid/lines.rs` (254) |
| 6 | orthogonal writing modes on abspos grid items | 17 | the css-position writing-mode gap, seen from grid | no owner yet (writing-mode lot) |
| 7 | z-index order of overlapping grid items | 13 (`grid-inline-z-axis-ordering-*`) | paint order, not layout | `paint/stacking.rs` |
| 8 | scrollbar reserving space from tracks | 4 | not located | — |
| 9 | `grid-areas-overflowing-grid-container-00N` (0.98 % each), `flex-sizing-rows-indefinite-height` (52.6 %) | ~10 | not located; the last one is an intrinsic-sizing gap worth its own look | — |

Not examined: ~380 of the 507 non-lanes failures, singletons and small groups
in `alignment/` and `grid-items/`.

## The 34 pass-empty

`grid-model` 19, `grid-lanes` 8, `grid-items` 3, root 3, `subgrid` 1. They are
the text gap (the raster paints no non-Ahem glyphs on either side), not grid
passes; they leave the column when the text crate lands.

## Order

1. Cause 1 (after the css-position static-position lot lands, since both edit
   `static_position.rs`). 2. Cause 2 (`aspect-ratio` into `grid/tracks.rs`,
   reusing what `block.rs` already resolves). 3. Cause 4 with the same files.
4. Subgrid, its own plan. `grid-lanes` is not on the list until Blink ships it.
