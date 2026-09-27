# rts-text

Fonts, shaping and glyph coverage for the CSS engine. Built from
`docs/superpowers/plans/2026-09-26-text-crate.md` (the FORM, F1–F5); what
comes next is `PLAN.md`.

| module | answers |
|---|---|
| `fonts/` | `FontStore::resolve(family list, weight, style) -> Option<Face>`; `register(bytes, family)` |
| `face.rs` | upem, ascent/descent/line gap with Blink's rounding, `units_to_px` |
| `shape.rs` | `shape` / `shaped_width` through `rustybuzz`, and the run cache (`cache_stats`) |
| `breaks.rs` | UAX #14 opportunities and grapheme boundaries, as byte indices |
| `raster.rs` | `rasterise(face, glyph, size) -> Bitmap` (u8 coverage + bearing) |
| `adapter.rs` | `RealMeasurer: rts_dom::layout::TextMeasurer`, feature `dom-measurer` |

## Rules

1. **This crate owns fonts, shaping, break opportunities and glyph coverage —
   and depends on nothing of this workspace.** Its dependencies are
   `rustybuzz` (and its `ttf_parser`), `skrifa` (pinned to epaint's),
   `unicode-linebreak`, `unicode-segmentation`. The ONE exception is the
   optional `dom-measurer` feature, which adds `rts-dom` for the
   `TextMeasurer` trait only, so the window and the headless instrument share
   one implementation (plan, F2).

2. **It never decides CSS.** What `white-space` keeps, which opportunity a
   line takes, what a `font-family` keyword means beyond Blink's Windows
   defaults, Ahem's unrounded metrics — those are `rts-dom`'s. This crate
   answers questions in its own terms (families, faces, runs, glyphs,
   bitmaps) and the adapter asks `rts-dom` where a CSS rule is involved.

3. **No font is ever committed.** Times New Roman, Arial, Consolas and Segoe
   UI come from the system directory; Ahem from the WPT checkout, registered
   by whoever has it. A test that needs a file it cannot find prints
   `SKIPPED:` and the reason — it never asserts nothing silently.

4. **Nothing found is `None`, never a guess.** A family list that resolves
   to no face answers `None`; the fallback belongs to the caller, and
   `RealMeasurer`'s is PER CALL: exactly what `ApproxMeasurer` answers for
   the same arguments, so a machine without the fonts lays out as it did
   before this crate existed.

5. **One source, and the generated tables are its check.**
   `rts-dom/src/layout/measure/font_metrics.rs` and `font_advances.rs` are
   what Edge MEASURED. `tests/blink_tables.rs` loads the real faces and
   reproduces them (28 metric rows, advances regular and bold). When the two
   disagree, the table is right and this crate is wrong — the tables are not
   regenerated from here.

6. **Files ≤ 500 lines**, the workspace ceiling outside the two engine
   crates.
