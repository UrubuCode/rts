# rts-text — plan

The source is `docs/superpowers/plans/2026-09-26-text-crate.md`; this file
only points at its tasks and records what T1 found.

| task | state | where the detail is |
|---|---|---|
| T1 — the crate | done (uncommitted at hand-off) | this crate; rulers in `tests/` |
| T2 — the instrument paints glyphs | open | plan, T2 + F3: `claude-raster` registers `Ahem.ttf` from an env var, measures with `RealMeasurer`, composites `rasterise` output, prints the masked count |
| T3 — the window measures with `RealMeasurer` | open | plan, T3 + F4; `rts-egui/src/app/fonts.rs` hardcodes Windows font paths and can ask `FontStore` instead |
| T4 — the inline layout consumes `breaks.rs` | open | plan, T4; `line_break_partition.rs:67` |
| T5 — `@font-face` | open, its own plan | `FontStore::register` is the loader it calls |

## What T1 found that the tables' documentation does not say

- **The vertical metrics are DirectWrite's, not `hhea`'s.** Consolas' `hhea`
  is 1521/−527/gap 350; the measured row (15/4 at 16px, gap 0) is
  `usWinAscent`/`usWinDescent` with gap `max(0, hhea extent − win extent)`.
  Times, Arial and Segoe UI have equal hhea and win values, which is why the
  header of `font_metrics.rs` could say `hhea` and still reproduce them. The
  Consolas constants there are right; the word is not. `face.rs` has the rule.
- **Kerning in "AVATAR Toy To." at 16px serif is 10px, not 14.** Edge 153:
  112.21875 kerned, 122.21875 with `font-kerning: none`; `shape.rs` gives
  both within 1/64px.

## Not done, and why

- **No system directory off Windows.** `FontStore::new()` has none there, so
  every resolve is `None` and `RealMeasurer` answers `ApproxMeasurer`'s
  numbers. Linux/macOS directories come when a target needs them.
- **The `USE_TYPO_METRICS` arm is untested** by a measured row: none of the
  four defaults sets the bit.
