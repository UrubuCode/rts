# `rts-text` — real text for the CSS engine (item 4 of the order)

**Written 2026-09-26** from the tree at `1ff2031bd`, after reading the audit
(`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/05-texto-e-fontes.md`),
PLAN.md rows `FM`, `FM-H`, `T`, `REGUA`, `TEXTO`, and the two triages of
2026-09-25. It is the plan an agent executes; the rules are
`crates/rts-dom/PLAN.md` §1–§2, `docs/ui/html-engine/box-tree.md` §7 and
`CLAUDE.md`.

## Corrections measured by T1 (2026-09-26)

- **Blink's vertical metrics are `usWinAscent`/`usWinDescent` of `OS/2`, not `hhea`**: Consolas gives 7 against 9 at 10 px from `hhea`; the line gap that reproduces all four fonts is `max(0, (hheaAsc − hheaDesc + hheaGap) − (winAsc + winDesc))`. Times, Arial and Segoe UI have identical `hhea` and win values, which is why the old header could say `hhea` and still be right for them. `USE_TYPO_METRICS` has no measured row (none of the four sets it).
- **Kerning of "AVATAR Toy To." at 16 px in Times is 10 px, not 14** (Edge 153: 112.21875 kerned, 122.21875 unkerned; rts-text 112.2188 / 122.2109).
- **The instrument's revealed set (T2):** 51 WPT tests passed only because their text was masked on both sides; painted, they differ (bidi, mixed-size baselines, wrapping beside floats, prose that differs between test and ref). They are layout findings, not paint regressions, and the ruler's base moves to the painted instrument from here.
- **`DisplayItem::Text` carries no family**, so the raster infers it from the measurer's calls; 280 WPT items and 28 corpus items stay masked as "family unknown". A family (or face id) on the item is T2b.

## What is true today, measured

- **Measurement** goes through one trait, `TextMeasurer`
  (`layout/measure/text_measurer.rs`, 176 lines). Headless, `ApproxMeasurer`
  answers from two GENERATED tables: `font_metrics.rs` (real `hhea` ascent,
  descent and line gap of the four fonts Blink resolves the generic families
  to on Windows — Times New Roman, Arial, Consolas, Segoe UI — with Blink's
  rounding, 28/28 rows reproduced) and `font_advances.rs` (real `hmtx`
  advances per character of the same fonts, regular and bold, measured by
  `canvas.measureText` in Edge at 2 048 upem). No kerning (`"AVATAR Toy To."`
  is 14 px narrower in real Times), no ligatures, no bidi, no `@font-face`,
  line breaks at `char` boundaries not grapheme clusters
  (`line_break_partition.rs:67`).
- **Windowed painting** is real: `rts-egui/src/frame/render/pintura.rs:167`
  calls `painter.text(...)`, and egui rasterises glyphs through `epaint`
  (`skrifa` 0.40 in `Cargo.lock`). Its measurer, `EguiMeasurer`
  (`rts-egui/src/frame/render/medida.rs`, 139 lines), asks egui's font
  system for widths and ascent and has no descent (epaint has none), so the
  windowed measurement and the headless one are two different numbers for
  the same text.
- **The instrument does not paint text.** `claude-raster.rs` (753 lines)
  skips every `DisplayItem::Text` except Ahem, which it fills as solid
  rectangles (`fill_ahem_text`, lines 347–363); everything else becomes a
  rectangle in `.mask.json` that `scripts/wpt_reftests.mjs` excludes from
  the comparison. That is why **≈24 % of the WPT "passes" are empty** (both
  sides blank, PLAN row `REGUA`: 1 369 of 5 615 sampled) and ≈13 % of the
  failures have only one side painting. `css-text/white-space`: 201 of the
  234 failures use Ahem, 33 need real glyphs; `css-position`: 5 empties,
  and any test with real text shows nothing.
- **Ahem** is handled by family NAME only (`style/ahem.rs`, 79 lines): no
  font file is read. No `Ahem.ttf` is in the repo; the WPT checkout's
  `fonts/` folder is not in the sparse-checkout.
- Already in `Cargo.lock`: `skrifa` (transitive of epaint), `unicode-segmentation`
  (transitive, unused by rts-dom). Not present: `rustybuzz`, `swash`,
  `ab_glyph`, `fontdue`, `cosmic-text`, `parley`, `unicode-linebreak`,
  `unicode-bidi`.

## The FORM, fixed first

**F1. One crate, `crates/rts-text/`, owns fonts, shaping and glyph
rasterisation, and depends on no crate of this workspace.** Its API is in
its own terms (families, faces, runs, glyphs, bitmaps), not in rts-dom's:

```
rts-text
  fonts/    FontStore: resolves a CSS family list + weight + style to a Face.
            Sources, in order: faces registered by the host (an `@font-face`
            later, Ahem now — the bytes handed in by whoever has them), then
            the system directory (Windows: %WINDIR%\Fonts; the four Blink
            defaults by file name, then any other family by its `name` table),
            then the generic fallback (serif→Times New Roman, sans-serif→Arial,
            monospace→Consolas, system-ui→Segoe UI). A family that resolves
            to nothing answers None — never a guess.
  face.rs   Face: upem, ascent/descent/line-gap from `hhea` (and `OS/2`
            typo/win metrics, which Blink prefers when USE_TYPO_METRICS is
            set), the same rounding `font_metrics.rs` documents, so the
            28-row Blink table is reproduced FROM THE FILE.
  shape.rs  shape(face, text, size, features) -> Vec<Glyph{ id, x_advance,
            x_offset, y_offset, cluster }> through `rustybuzz` — kerning and
            ligatures on by default as CSS `font-kerning: auto` means.
  breaks.rs UAX #14 line-break opportunities (`unicode-linebreak`) and
            grapheme clusters (`unicode-segmentation`), answered as byte
            indices of the run's text — the inline layout stays the caller.
  raster.rs rasterise(face, glyph, size) -> coverage bitmap (u8 alpha) and
            its bearing, through `skrifa`'s outline + a scanline fill of our
            own (no `swash`: one more crate for what forty lines do; if the
            forty lines are not enough, `swash` is the named alternative).
```

Dependencies of `rts-text`: `rustybuzz`, `ttf-parser` (rustybuzz re-exports
it), `unicode-linebreak`, `unicode-segmentation`, `skrifa`. Nothing else.
`rts-dom` gains ZERO dependencies (PLAN row `T`): it keeps the trait and
`ApproxMeasurer` as the headless fallback.

**F2. One implementation of `TextMeasurer` over `rts-text`, used by BOTH the
window and the instrument.** `crates/rts-text/src/measurer.rs` is not
possible without depending on rts-dom, so the adapter lives in a tiny
crate-free place both can reach: `crates/rts-dom/src/layout/measure/real.rs`
is ruled out by F1 (zero deps). Therefore: **`rts-egui/src/frame/render/measure.rs`
replaces `EguiMeasurer` with `RealMeasurer` over `rts-text`** (rts-egui
depends on rts-text), and **`claude-raster` uses the same `RealMeasurer`
through `rts-egui`**? No — the raster must stay a headless example of
rts-dom without a window. So `RealMeasurer` is defined in
`crates/rts-text/src/adapter.rs` behind a cargo feature `dom-measurer` that
adds an optional dependency on `rts-dom` for the trait only. rts-egui and the
example enable the feature; `rts-text` without it depends on nothing of ours.
Stated so the dependency is visible and optional, not hidden.

`RealMeasurer` answers `text_width` by shaping (kerning included),
`line_height`/`font_ascent`/`font_descent` from the face with Blink's
rounding, `identity()` as a hash of the loaded faces so the layout cache key
changes when fonts change. When a family resolves to no face, it answers
what `ApproxMeasurer` answers today, per call, so a machine without the font
(CI on Linux) degrades to the current behaviour rather than to zero.

**F3. The instrument paints glyphs.** `claude-raster` composites the
coverage bitmap of every glyph of a `DisplayItem::Text` (colour × alpha) at
the shaped positions, for every family `rts-text` resolves; Ahem stays a
solid rectangle from the same face (Ahem's glyphs ARE squares, so the
result is identical, and the face is the WPT `fonts/Ahem.ttf`, added to the
sparse-checkout — registered into the `FontStore` by the runner, never
embedded in the repo). Text that resolves to no face keeps the mask, as
today, and the report says how many items were masked — that number is the
new ruler for "how much of the corpus the instrument can now see".

**F4. The window measures with the same numbers it paints.** rts-egui's
painter keeps epaint's galley for drawing (this lot does not replace egui's
renderer), but its `TextMeasurer` is `RealMeasurer`, so `getBoundingClientRect`
and the line breaks are the same in the window and in the raster. The
divergence between epaint's glyph advances and rustybuzz's is measured by
the paint-parity ruler (177 PNGs) and stated; if it is above noise, the
painter's per-glyph positions come from `rts-text`'s shaping in a follow-up
(a `DisplayItem::Text` gains no field in this lot).

**F5. The generated tables stay as the fallback, and the real face must
reproduce them.** A test in `rts-text` loads Times New Roman, Arial,
Consolas and Segoe UI from the system directory and asserts the 28 rows of
`font_metrics.rs` and a sample of `font_advances.rs` (regular and bold)
byte for byte. That is the proof the two sources agree; when they do not,
the generated table is what Blink measured and the loader is wrong.

## Rulers

1. `cargo test -p rts-text`: metrics and advances against the generated
   tables (F5); shaping of `"AVATAR Toy To."` in Times narrower than the
   sum of advances by the kerning Blink shows (measure the exact px in Edge
   with `scripts/css_fixtures_medir_edge.mjs` — it takes the OUTPUT file as
   argument — before writing the number); UAX #14 opportunities on a fixed
   string; a grapheme cluster (`é`, an emoji ZWJ sequence) never split.
2. **Corpus `tests/css` via `claude-css-runner` (168/170, Edge-measured
   rects) per file: LOST none.** Real shaping changes widths toward Blink's;
   a lost fixture here is a real number to explain, not to accept.
3. **WPT, the six folders + `css-grid` + `css-sizing`, per file, TWO
   comparisons stated separately:** (a) pass→fail on tests that passed
   NON-empty before = LOST, must be empty; (b) `passa-vazio` → `passa` or
   `falha` is the point of the lot and is reported as its own table (empties
   before/after per folder, and how many became real passes).
4. Paint parity 177/177 in rts-egui (F4), with the per-fixture pixel deltas
   where they are not identical, and the reason.
5. Suite 890/905, LOST none (rts-egui is on the path of the UI tests).
6. `cargo run --release -p rts-dom --example dom_metrics scripts/parity/pagina.combinada.html`
   before/after: layout time with `RealMeasurer` must not exceed the
   `ApproxMeasurer` number by more than the shaping cost stated per run
   (measure; a cache of shaped runs keyed by `(face, size, text)` inside
   `rts-text` is the expected answer, and its hit rate is printed).

## Tasks

- [x] **T1 — the crate** (one agent, Opus): `rts-text` with `fonts/`,
  `face.rs`, `shape.rs`, `breaks.rs`, `raster.rs`, the `dom-measurer`
  feature with `adapter.rs::RealMeasurer`, and the F5 tests. Workspace
  member, README of six rules or fewer (what the crate owns, what it never
  decides — CSS is rts-dom's), files ≤ 500 lines. `cargo test -p rts-text`
  and `cargo check -p rts-text --features dom-measurer`.
- [x] **T2 — the instrument** (one agent, after T1 is merged): `claude-raster`
  loads Ahem from `Documents/wpt-corpus/css/fonts/Ahem.ttf` (path from an env
  var the runner sets; `scripts/wpt_reftests.mjs` passes it), measures with
  `RealMeasurer`, paints glyphs, keeps the mask for unresolved families,
  prints the masked count. Ruler 3 in full, both tables.
- [x] **T2b — the family on `DisplayItem::Text`** (2026-09-26): masked text 280 → 0, +6 WPT; `bare_text.rs` measured without a family and with bold/mono swapped, fixed.
- [ ] **T3 — the window** (one agent, after T1): `EguiMeasurer` → `RealMeasurer`;
  rulers 4, 5, 6.
- [ ] **T4 — the inline layout consumes breaks and clusters** (after T2): the
  line breaker asks `rts-text` for UAX #14 opportunities and grapheme
  boundaries through the trait (two new trait methods with default
  implementations that keep today's behaviour), `line_break_partition.rs:67`
  stops cutting at `char`. Ruler 2 and 3 again.
- [ ] **T5 — `@font-face`** (its own plan): the loader exists (F1 "faces
  registered by the host"); the CSS side (`@font-face` parsing, `src: url()`
  through the resource loader, `unicode-range`) is the next lot.

## Constraints

- **Not in this plan:** bidi (UAX #9), vertical text (the writing-mode
  lot), `font-variant-*`, colour fonts, replacing egui's renderer.
- Fonts are never committed to the repo: Times/Arial/Consolas/Segoe UI are
  Windows' and Ahem is the WPT checkout's. A machine without them runs the
  fallback and the report says so.
- One agent per task; T1 has no other agent touching `Cargo.toml` of the
  workspace at the same time. The coordinator builds release and runs
  rulers 2–6.
