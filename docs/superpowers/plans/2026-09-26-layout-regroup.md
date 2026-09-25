# `layout/` regrouped by formatting context — item 3 of the order of 2026-09-25

**Written 2026-09-26** from the tree at `645b19733`, after item 2 closed
(`2026-09-25-paint-and-query.md`, #2766 and #2769). It is the plan an agent
executes; the rules it points at are `crates/rts-dom/README.md`,
`crates/rts-dom/PLAN.md` §1–§2 and `docs/ui/html-engine/box-tree.md`. Item 4
of the order (the text crate) comes AFTER this and is not planned here.

## Why a regroup and not a split

`layout/` is 77 files and 18 678 lines, all FLAT beside `layout.rs` — the only
subfolder is `tests/`. A reader looking for "how a flex column wraps" has to
know that the answer is spread over `coluna.rs`, `coluna_wrap.rs`,
`coluna_wrap_largura.rs`, `coluna_shrink.rs`, `coluna_rtl.rs`, `flex.rs`,
`flex_limites.rs`, `flex_baseline.rs`, `flex_linhas.rs`, `flex_margens_auto.rs`,
`flex_basis_content.rs`, `flex_pseudo.rs`, `flex_stretch_replaced.rs` and
`eixos_flex.rs` — fourteen files whose only grouping is a name prefix, in two
languages. The same holds for the inline formatting context (fifteen files),
the block one (ten) and measurement (twelve). `layout.rs` declares 75 `mod`
lines in no order a reader can use.

The coupling that matters is BETWEEN contexts: `bloco.rs` (1 277 lines) is the
dispatcher every context returns to, and `flex`, `grid`, `table` and the line
all call `layout_block`/`measure_block` for their children. A folder per
context makes that call graph visible as `super::` against `crate::layout::`;
today every edge is `super::` and nothing distinguishes an intra-context call
from a cross-context one.

What this plan does NOT do: split any file, change any signature, change any
`pub` visibility except where a move forces a path (below), touch
`layout/tests/` beyond the paths it imports, or rename an IDENTIFIER. The six
files above the 500-line ceiling (`bloco.rs` 1 277, `vertical.rs` 744,
`flex.rs` 597, `coluna_wrap.rs` 551, `coluna.rs` 546, `fragmento.rs` 515)
are moved as they are; splitting them is logic-adjacent work with its own
ruler and comes after, per folder, once the folder says what belongs together.

## The FORM, fixed first

**One folder per formatting context; a file's folder is decided by the
context whose OUTPUT it produces, not by the context that calls it.** Where a
file serves two (a pseudo-element box laid out as a flex item, a float placed
in the middle of a line), it goes with the context it EMITS INTO — the folder
whose fragment it appends to. The table below is the decision, made once;
an agent does not re-decide it per file.

```
layout.rs                 the hub: LayoutCtx, layout_document, layout_cached,
                          bounding_rect, the pub use lines, the mod tree
layout/
  block/                  the block formatting context: a block's box, the
                          vertical flow, margin collapse, BFC, the child sequence
  inline/                 the inline formatting context: runs, line breaking,
                          segments, baselines, vertical-align, hyphens, tabs
  flex/                   flex row and column, wrap, basis, baseline, axes
  grid/                   grid tracks, placement, collapse
  float/                  floats: exclusions, placement, float-in-line, clearfix
  positioned/             absolute/fixed/relative and the static position
  replaced/               <img>, <svg>, <canvas>, <input>, <select>: their
                          intrinsic and transferred sizes
  measure/                intrinsic sizes and the text measurer: min/max-content,
                          font metrics and advances, the active measurer
  fragment/               the OUTPUT: Fragment, ChildRef, BoxRects, item
                          transforms, the stitching questions, transform_rects
  tests/                  unchanged
```

**Naming: the move renames each file to English**, because a file is touched
once by this lot and the repo writes new identifiers in English (#2758 renamed
that day's; the rest were left for "when the file is next moved", and this is
that moment). Identifiers inside a file are NOT renamed — `fn juntar!` stays
`juntar!` — because an identifier rename is a diff a reviewer cannot tell from
a logic change, and the ruler of this lot is "byte-identical output", which a
rename of a FILE cannot break and a rename of a SYMBOL can.

The table, complete. Every file directly under `layout/` today is in it once;
a file not in it is an error in the plan, not a free choice.

| today | becomes | context |
|---|---|---|
| `bloco.rs` | `block/block.rs` | block |
| `bloco_caixa.rs` | `block/block_box.rs` | block |
| `vertical.rs` | `block/vertical_flow.rs` | block |
| `sequencia.rs` | `block/sequence.rs` | block |
| `bfc.rs` | `block/bfc.rs` | block |
| `bfc_estilo.rs` | `block/bfc_style.rs` | block |
| `bfc_evita_float.rs` | `block/bfc_avoids_float.rs` | block |
| `margem_escapada.rs` | `block/escaped_margin.rs` | block |
| `rtl_bloco.rs` | `block/rtl.rs` | block |
| `overflow_viewport.rs` | `block/overflow_viewport.rs` | block |
| `pseudo_bloco.rs` | `block/pseudo_block.rs` | block (emits a block box) |
| `pseudo_caixa.rs` | `block/pseudo_box.rs` | block (box model of a generated box) |
| `texto_solto.rs` | `block/bare_text.rs` | block (a text node reaching the block flow) |
| `caixa.rs` | `block/box_kind.rs` | block ("which box is this" is asked by the block dispatcher) |
| `linha.rs` | `inline/line.rs` | inline |
| `linha_atomos.rs` | `inline/line_atoms.rs` | inline |
| `linha_baseline.rs` | `inline/line_baseline.rs` | inline |
| `linha_ib.rs` | `inline/line_inline_block.rs` | inline |
| `alinhamento_vertical.rs` | `inline/vertical_align.rs` | inline |
| `runs.rs` | `inline/runs.rs` | inline |
| `quebra.rs` | `inline/line_break.rs` | inline |
| `quebra_particao.rs` | `inline/line_break_partition.rs` | inline |
| `preserved_spaces.rs` | `inline/preserved_spaces.rs` | inline |
| `segmento.rs` | `inline/segment.rs` | inline |
| `hifen.rs` | `inline/hyphen.rs` | inline |
| `tabulacao.rs` | `inline/tab_size.rs` | inline |
| `inline_fragmentos.rs` | `inline/inline_fragments.rs` | inline (emits line fragments) |
| `pseudo_inline.rs` | `inline/pseudo_inline.rs` | inline |
| `fonte_do_trecho.rs` | `inline/run_font.rs` | inline (the font is per inline box) |
| `ancora_estatica.rs` | `inline/static_anchor.rs` | inline (an atom on the line; `positioned/` reads it) |
| `flex.rs` | `flex/row.rs` | flex |
| `coluna.rs` | `flex/column.rs` | flex |
| `coluna_wrap.rs` | `flex/column_wrap.rs` | flex |
| `coluna_wrap_largura.rs` | `flex/column_wrap_width.rs` | flex |
| `coluna_shrink.rs` | `flex/column_shrink.rs` | flex |
| `coluna_rtl.rs` | `flex/column_rtl.rs` | flex |
| `flex_limites.rs` | `flex/limits.rs` | flex |
| `flex_baseline.rs` | `flex/baseline.rs` | flex |
| `flex_linhas.rs` | `flex/lines.rs` | flex |
| `flex_margens_auto.rs` | `flex/auto_margins.rs` | flex |
| `flex_basis_content.rs` | `flex/basis_content.rs` | flex |
| `flex_pseudo.rs` | `flex/pseudo.rs` | flex |
| `flex_stretch_replaced.rs` | `flex/stretch_replaced.rs` | flex |
| `eixos_flex.rs` | `flex/axes.rs` | flex |
| `dimensao_indefinida.rs` | `flex/indefinite_size.rs` | flex (only `flex.rs` and the hub read it) |
| `grid.rs` | `grid/grid.rs` | grid |
| `grid_linhas.rs` | `grid/lines.rs` | grid |
| `grid_tracks.rs` | `grid/tracks.rs` | grid |
| `grid_colapso.rs` | `grid/collapse.rs` | grid |
| `float.rs` | `float/float.rs` | float |
| `float_placement.rs` | `float/placement.rs` | float |
| `float_in_line.rs` | `float/in_line.rs` | float (emits the exclusion; the line only reads) |
| `clearfix.rs` | `float/clearfix.rs` | float |
| `posicionado.rs` | `positioned/positioned.rs` | positioned |
| `posicao_estatica.rs` | `positioned/static_position.rs` | positioned |
| `caixa_contentora.rs` | `positioned/containing_block.rs` | positioned |
| `relativo.rs` | `positioned/relative.rs` | positioned |
| `replaced.rs` | `replaced/replaced.rs` | replaced |
| `replaced_transferido.rs` | `replaced/transferred_size.rs` | replaced |
| `input.rs` | `replaced/input.rs` | replaced |
| `input_sizing.rs` | `replaced/input_sizing.rs` | replaced |
| `select.rs` | `replaced/select.rs` | replaced |
| `medida.rs` | `measure/measure.rs` | measure |
| `medida_arvore.rs` | `measure/tree.rs` | measure |
| `tamanho_intrinseco.rs` | `measure/intrinsic_size.rs` | measure |
| `intrinseco_min_max.rs` | `measure/intrinsic_min_max.rs` | measure |
| `text_measure.rs` | `measure/text.rs` | measure |
| `medidor_texto.rs` | `measure/text_measurer.rs` | measure |
| `medidor_ativo.rs` | `measure/active_measurer.rs` | measure |
| `fonte_metricas.rs` | `measure/font_metrics.rs` | measure |
| `fonte_avancos.rs` | `measure/font_advances.rs` | measure (GENERATED by `scripts/fonte_avancos_gera.cjs`; its output path changes too) |
| `fragmento.rs` | `fragment/fragment.rs` | fragment |
| `fragmento_tipos.rs` | `fragment/types.rs` | fragment |
| `box_fragments.rs` | `fragment/box_rects.rs` | fragment |
| `itens.rs` | `fragment/items.rs` | fragment |
| `costura_filhos.rs` | `fragment/stitching.rs` | fragment |
| `transform_rects.rs` | `fragment/transform_rects.rs` | fragment |

Seventy-seven rows, checked against `ls layout/*.rs` on 2026-09-26; `layout.rs` and `tests/` stay. If an agent finds a file on
disk that is not in this table, it stops and says so; it does not place it.

**`layout.rs` after the move** declares nine `pub(crate) mod` folders and
keeps exactly the `pub use` lines it has now (`ChildRef`, `Fragment`,
`BoxRects`, `ApproxMeasurer`, `TextMeasurer`) plus the free functions it
defines itself. Each folder's `mod.rs` declares its files and re-exports, with
`pub(crate) use`, the names its siblings and the hub reach today via
`crate::layout::X` — so that every `crate::layout::X` in the crate keeps
compiling with the SAME path. That is the rule that makes this a move and not
a refactor: **no call site outside `layout/` changes for a name that was
re-exported before**.

**What DOES change outside `layout/`: six modules reached by their module
PATH** rather than by a re-exported name (the survey counted 17 call sites, in
`boxes/context.rs`, `dom/estilo.rs`, `dom/geometria.rs`, `dom/tests/`,
`paint/list.rs`, `paint/pieces.rs`, `style/values/dimensao.rs`,
`table/widths/mod.rs`, and cross-crate `rts-egui/src/frame/render/mod.rs`):

| today | after |
|---|---|
| `layout::medidor_ativo::{with_active, set_active, clear_active}` | `layout::measure::active_measurer::…` — and `layout.rs` adds `pub use measure::active_measurer;` so `rts_dom::layout::active_measurer::set_active` is the cross-crate path |
| `layout::itens::translate_item` | `layout::fragment::items::translate_item` |
| `layout::fonte_metricas::FontMetricsModel` | `layout::measure::font_metrics::FontMetricsModel` |
| `layout::text_measure::intrinsic_text_width` | `layout::measure::text::intrinsic_text_width` |
| `layout::bloco::establishes_block_formatting_context` | `layout::block::establishes_block_formatting_context` (re-exported by `block/mod.rs`) |
| `layout::bfc_estilo::{pelo_estilo, overflow_estabelece}` | `layout::block::bfc_style::…` |

Inside `layout/`, every `super::x::` between two files that land in DIFFERENT
folders becomes `crate::layout::<folder>::x::` (or the re-exported
`crate::layout::X`), and every one between two files of the SAME folder stays
`super::`. `use super::*;` at the top of a moved file — most files have it —
keeps resolving to the folder's `mod.rs`, so **each `mod.rs` re-exports, with
`use super::*` semantics in mind, what its files reached through the old
`super::*`**: the hub's `pub use` names, `LayoutCtx`, and the sibling helpers.
The agent gets this right by compiling `cargo check -p rts-dom --tests` after
EACH folder, not at the end.

## The state this plan starts from (read 2026-09-26)

- 77 flat files + `layout.rs` (487 lines) + `tests/` (97 files, `mod.rs` 360
  lines of `mod` declarations and shared helpers).
- `layout.rs` declares `pub mod medidor_ativo` (the only `pub mod`) and
  `pub(crate) mod {caixa, fonte_metricas, text_measure, bloco}`; everything
  else is private `mod`.
- Consumers outside the crate: `rts-egui/src/frame/render/mod.rs`
  (`layout::{self, TextMeasurer}`, `layout::medidor_ativo::{set_active,
  clear_active}`), `rts-dom/examples/claude-raster.rs`,
  `rts-dom/examples/dom_metrics/scenarios.rs` (`ApproxMeasurer`, `LayoutCtx`,
  `TextMeasurer`, `layout_document`, `layout_cached`). `rts-dom-bridge` and
  `rts-ui` do not name a `layout::` path.
- `fonte_avancos.rs` is GENERATED by `scripts/fonte_avancos_gera.cjs` (from
  `scripts/fonte_avancos_medir_edge.mjs`); the generator's output path must
  follow the file or the next regeneration recreates the old path.
- The three dependency seams item 2 left are untouched by this plan and stay
  where they are: `paint::pieces` holds a layout `Fragment`, `paint::list`
  holds a `BoxRects` and a `query::Geometry`, `layout_document` reads
  `geometry_now`. They become `crate::layout::fragment::…` paths and nothing
  else.

## Rulers

The same four as item 2, per file against `target/baseline.exe` (= `main` at
`645b19733`) and the Phase B reports, because a move that changes an answer
is a move that changed logic:

1. **170/170 PNGs byte-identical**: `claude-raster` over `tests/css/*.html`,
   SHA-256 of PNG + mask, before (the kept `scratchpad/pb/hashes.txt`) and
   after. Zero different, zero missing.
2. **WPT six folders 0/0**: `scripts/wpt_reftests.mjs` over `css-flexbox`,
   `CSS2/normal-flow`, `css-grid/alignment`, `CSS2/floats`,
   `css-text/white-space`, `css-position`, compared with
   `scripts/wpt_comparar.mjs` against `Documents/wpt-pb`. LOST none, GAINED
   none, OTHER none — a GAIN is as suspicious as a loss in a lot with no logic.
3. **Corpus**: `examples/claude-css-runner.ts` output equal to
   `$TEMP/corpus-base.txt` modulo timings (168/170, 4 525/4 528 measurements).
4. **Suite**: `medir.sh` against `$TEMP/suite-base.txt` (890/905), LOST none.
5. **`cargo test -p rts-dom --lib --no-fail-fast`: 1 313 passed with the SAME
   sorted list of test names** — a moved `#[cfg(test)]` module keeps its
   tests; a lost test file is the failure this line exists to catch.
6. `cargo check -p rts-egui -p rts-dom-bridge -p rts-ui -p rts-dom --tests
   --examples` clean, no NEW warning (the 15 existing unused-import warnings
   are listed by path in the item-2 session; a warning at a new path is new).

## Tasks

One agent, one worktree, one lot. The folders are done in the order below
because each one's `mod.rs` needs the previous folders' paths to exist, and
the order goes from the leaves of the call graph (measure, fragment) toward
the hub (block). `cargo check -p rts-dom --tests` after each folder; the
release build and the rulers ONCE at the end by the coordinator.

- [ ] **R1 — `measure/` and `fragment/`.** The two leaf folders. Create
  `mod.rs` for each, `git mv` the twelve + six files to their English names,
  rewrite `super::x::` cross-folder paths, add the `pub use` for
  `active_measurer` in `layout.rs`, fix the 17 module-path call sites, fix
  the output path in `scripts/fonte_avancos_gera.cjs`. Check.
- [ ] **R2 — `replaced/`, `positioned/`, `float/`, `grid/`.** Four small
  folders, same procedure. Check after each.
- [ ] **R3 — `flex/` and `inline/`.** The two large ones (14 and 16 files).
  Check after each.
- [ ] **R4 — `block/`, and `layout.rs` reduced to the hub.** Fourteen files;
  then `layout.rs` holds nine `pub(crate) mod` lines, its own functions, and
  the five `pub use` lines. `layout/tests/mod.rs` paths updated last. Check
  the whole workspace with `--tests --examples`; `cargo test -p rts-dom --lib`
  and diff the sorted test-name list against the one taken before R1.
- [ ] **R5 — docs and the PLAN row** (coordinator, after the rulers):
  `docs/ui/html-engine/box-tree.md` names ten `layout/` files — update the
  paths (`rts-dom` has no README; its rules are PLAN.md §1); PLAN.md §0 gains a
  `LR` row with the four rulers' numbers; the stale comments item 2 listed
  (`boxes/generated.rs:105`, `style/effects.rs:131`, `style/borders.rs:303`,
  `style/values/texto.rs:156`) are corrected in the same commit since their
  targets move again here.

## Constraints

- **Moves and path rewrites only.** No line of a function body changes. If
  a move needs a body change to compile (a `super::super::` that cannot be
  expressed, a private item a sibling folder needs), the agent widens the
  visibility to `pub(crate)` or adds a `pub(crate) use` in a `mod.rs`, and
  lists every such widening in its report — as item 2's agent did.
- **`git mv`, one file per move**, so `git log --follow` survives. Item 2
  used plain file operations and lost the follow; this lot does not.
- **No identifier renames**, for the reason stated under FORM. The Portuguese
  identifiers inside moved files are the NEXT lot's, per folder, with the
  folder's own ruler.
- **One agent.** Two agents moving files in one crate is the BT-3 mistake the
  epic's comment of 2026-09-18 describes. The agent does not run `cargo
  build --release`, the rulers, or `git commit`; it hands back a worktree
  with a clean `cargo check --tests --examples` and the widening list.
- **`tests/` is not regrouped.** 97 test files under `layout/tests/` are
  reached by `layout/tests/mod.rs` and are named by what they pin, not by
  context; their `use crate::layout::…` lines are rewritten and nothing else.
