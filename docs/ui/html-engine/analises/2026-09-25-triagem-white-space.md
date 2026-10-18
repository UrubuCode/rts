# Triage: `css-text/white-space` WPT reftests (2026-09-25)

**What was measured.** `C:\Users\nexga\Documents\wpt-bt2a\css-text_white-space\relatorio.json`,
produced by `scripts/wpt_reftests.mjs` against the corpus at
`C:\Users\nexga\Documents\wpt-corpus\css\css-text\white-space\`. The base binary
is `target/base/release/examples/claude-raster.exe`, built from `main` at
`36c1545d2`. The prompt that ordered this triage states the branch under review
(`feat/dom-bt2a-caixa-obrigatoria`, `ee36cda2a`) measures identically to that
base on this folder — this triage did not itself rebuild or re-run the branch,
only read the one report and rasterized individual files with the base binary,
per the read-only mandate.

**Totals** (tol=8): 353 reftests. 89 `passa` (pass with content), 30
`passa-vazio` (both sides blank — proves nothing, issue #2729), 230 `falha`, 4
`falha-parcial` (one side paints, the other does not). This triage covers the
234 `falha`+`falha-parcial`.

## 1. Ahem / non-Ahem split

Grepped every failing test's HTML source for `Ahem`/`ahem`:

| | files |
|---|---:|
| uses Ahem (font declared somewhere in the test) | 201 |
| does NOT use Ahem | 33 |

**The 33 non-Ahem failures are not measurable by this instrument today** — the
raster paints no glyphs for a non-Ahem font (`claude-raster.rs`,
`fill_ahem_text` only fires when `DisplayItem::Text::is_ahem` is true; every
other text item is pushed into the comparison MASK, per `text_mask_rect`). A
failure in this group can only be trusted if the PNG shows a difference that
is not text (background, border, box geometry). List:

```
eol-spaces-bidi-002, line-edge-white-space-collapse-001, line-edge-white-space-collapse-002,
tab-bidi-001, text-wrap-balance-line-clamp-005, text-wrap-balance-line-clamp-007,
trailing-ideographic-space-001, trailing-ideographic-space-002, trailing-ideographic-space-004,
trailing-ideographic-space-005, trailing-ideographic-space-010, trailing-other-space-separators-001,
trailing-other-space-separators-003, trailing-other-space-separators-004,
white-space-intrinsic-size-022, white-space-intrinsic-size-023, white-space-letter-spacing-001,
white-space-normal-011, white-space-pre-031, white-space-pre-032, white-space-pre-034,
white-space-pre-035, white-space-pre-wrap-justify-003, white-space-pre-wrap-trailing-spaces-021,
white-space-wrap-after-nowrap-001, white-space-zero-fontsize-001, white-space-zero-fontsize-002,
ws-break-spaces-applies-to-003, ws-break-spaces-applies-to-008, ws-break-spaces-applies-to-009,
ws-break-spaces-applies-to-010, ws-break-spaces-applies-to-011, ws-break-spaces-applies-to-014
```

Two of these (`line-edge-white-space-collapse-001`, `white-space-intrinsic-size-022/023`)
are also in the `falha-parcial` set, so at least a geometry-level difference is
provable there without glyphs; the rest give this triage nothing beyond "blocked
on TEXTO" until re-measured. Note `ws-break-spaces-applies-to` has BOTH Ahem
members (in the n=512 group below) and non-Ahem members (003/008/009/010/011/014
above) — same test family, some numbers use Ahem and some do not; only the Ahem
ones were rasterized here.

## 2. Groups by exact pixel-difference count

`pct` in the report was converted to an absolute pixel count via the canvas
size implied by the `piores` entries in the same report, which give both `pct`
and `n` (e.g. `pct=6.34765625` ↔ `n=65000` ⇒ canvas ≈ 1 024 000 px). All 83
distinct pixel counts, largest groups first:

| n (pixels) | files | feature | cause (MEASURED / HYPOTHESIS) | evidence | lot |
|---:|---:|---|---|---|---|
| 800 | 22 | `break-spaces`/`break-spaces-tab` (word-break, tab) | **MEASURED**: `break-spaces-tab-003/004`, `break-spaces-004` rasterized. Idiom: an absolutely-positioned RED decoy (`z-index:-1`) sits under a GREEN test box that must wrap to exactly the same number of lines to hide it completely. Our engine wraps the tab/space run into fewer lines than expected — `break-spaces` is supposed to open a wrap opportunity after **each** tab/space character (CSS Text 3 §white-space-phase-1); ours treats the run as fewer break points, so the green box is shorter than the red decoy and a red strip shows through. | `ws_raster/break-spaces-004_{test,ref}.png`, `ws_raster/break-spaces-tab-003_{test,ref}.png` (both show 1 extra decoy row of red directly below/instead of the expected 2nd green square) | new — not in any `PLAN.md` lot (belongs in `layout/quebra.rs`'s per-character break-opportunity logic for `break-spaces`, not IFC as scoped today) |
| 1250 | 21 | `break-spaces-before-first-char-*`, `break-spaces-006/008` | **MEASURED** via `break-spaces-006` source: identical fail/test decoy idiom, `word-break: break-all` variant. Same root cause as the n=800 row — break-opportunity placement inside a `break-spaces` run. | source read, idiom matches | same as above |
| 625 | 16 | `break-spaces-003/010/011`, `break-spaces-before-first-char-007..009`, … | **HYPOTHESIS** (same file-name family, same idiom, not individually rasterized): same cause as n=800/1250 at a different font-size/ch-width so the leaked decoy strip is a different pixel count. | file-name pattern only (method's stated limit) | same as above |
| 1200 | 14 | `eol-spaces-bidi-003`, `trailing-other-space-separators-break-spaces-001..005`, … | **HYPOTHESIS**: `trailing-other-space-separators-break-spaces-*` combine trailing-space-hang with `break-spaces`; `eol-spaces-bidi-003` adds bidi. Likely the same break-opportunity cause, possibly compounded by a second one for the bidi member. | file-name pattern; `eol-spaces-bidi-003` not individually rasterized | new (break-spaces) + possibly bidi/LOG for the bidi member — unconfirmed |
| 2500 | 11 | `break-spaces-007`, `break-spaces-before-first-ideographic-char-001`, `break-spaces-with-ideographic-space-010`, `break-spaces-with-overflow-wrap-005/006`, `pre-wrap-leading-spaces-002`, … | **HYPOTHESIS**: same decoy idiom, ideographic/overflow-wrap variants of the same break-opportunity question. | file-name pattern | new (break-spaces) |
| 1600 | 10 | `break-spaces-tab-001/002`, `pre-wrap-001/003/004/005`, … | **HYPOTHESIS**: `pre-wrap-00x` are the reference targets `link rel=match`ed by many `break-spaces` tests above, and appear here as failing test files IN THEIR OWN RIGHT too (a `pre-wrap` test with its own decoy). Same idiom, same suspected cause: line count off by one wrap decision. | file-name pattern, cross-checked against `<link rel=match>` targets used above | new (break-spaces / pre-wrap wrapping) |
| 36160 | 9 | `trailing-space-and-text-alignment-00[1-9]`, `-rtl-001` | **MEASURED**: source is 5 `<textarea>`s (`left/center/right/start/end`) with Ahem font and a trailing-space-hang assertion. `layout/input.rs` hardcodes `is_ahem: false` on every `DisplayItem::Text` it emits (2 call sites, lines ~61 and ~355) — textarea text NEVER gets the Ahem flag threaded through, regardless of the actual `font-family`. So textarea text is always pushed into the raster's mask instead of painted, even where both sides of the comparison are textareas with identical text. The 9-file diff is not (necessarily) about `text-align`; it can come entirely from any asymmetric background/border trick around the invisible text. | `crates/rts-dom/src/layout/input.rs:61,355`; support CSS confirms `font: 40px/1 Ahem` on the textarea | TEXTO (glyph painting), but this specific defect is a *raster-instrument* gap distinct from "no font renderer" — `is_ahem` is not threaded on this one code path the way `texto_solto.rs`/`pseudo_caixa.rs` do it |
| 55000 | 8 | `break-spaces-051/052`, `pre-line-051/052`, `pre-wrap-051/052`, `white-space-pre-051/052` | **MEASURED**: `break-spaces-051` source read — `<span id="break-spaces">AB&NewLine;</span><span> CD</span>` inside a `float:left; background:red` box, matched against `ref-filled-green-100px-square.xht`. This is the SAME red-decoy idiom but with the decoy as the box's own background rather than a sibling `<div>`. Largest diffs in the whole corpus (5.37% of canvas): whatever line-feed/space-collapsing decision `break-spaces`/`pre-line`/`pre-wrap`/`pre` make around an embedded `&NewLine;` immediately followed by a collapsible space in the NEXT inline, our engine's result leaves a large uncovered red area — consistent with the box collapsing to fewer/narrower lines than the 100px reference square. | `ws_raster` not run on this file (time budget); source + idiom read | new (break-spaces/pre-line/pre-wrap/pre run-boundary collapsing across sibling inline elements) |
| 400 | 7 | `break-spaces-005`, `break-spaces-with-ideographic-space-004`, `pre-wrap-014`, `trailing-ideographic-space-011/015/016`, … | **HYPOTHESIS**: same idiom family. | file-name pattern | new (break-spaces) |
| 694 | 7 | `textarea-pre-wrap-001..007` | **MEASURED**: `textarea-pre-wrap-001` read — Ahem `<textarea>`, `white-space: pre-wrap`, a `background: linear-gradient(red,red) 0 0/2ch 2ch no-repeat` decoy behind the text, matched against a reference textarea using `white-space: pre` (no background trick). Root cause is the SAME `layout/input.rs` `is_ahem: false` bug identified above: the green Ahem text that is supposed to cover the red gradient patch is never painted (masked out) in either side, but only the TEST side has a red patch to begin with, so it always shows through. This is the single largest concretely-code-confirmed defect in this triage. | `crates/rts-dom/src/layout/input.rs:61,355`; `textarea-pre-wrap-001.html` + its `-ref.html` read side by side | TEXTO (same instrument gap as the n=36160 row) |
| 5000 | 7 | `break-spaces-009`, `break-spaces-with-overflow-wrap-009/010`, `pre-wrap-leading-spaces-001/015/016`, … | **HYPOTHESIS**: same break-opportunity idiom family. | file-name pattern | new (break-spaces) |
| 512 | 5 | `ws-break-spaces-applies-to-008/009/010/011/014` | **HYPOTHESIS**: same family name also has non-Ahem members (007? and the 003/008-014 range partially overlaps the non-Ahem list above — 008/009/010/011/014 ARE in the non-Ahem list too, meaning this specific n=512 group's *actual* Ahem members must be a different subset than what grepped "non-Ahem"; **this triage could not resolve that contradiction** — see §6). | file-name pattern; contradicts the earlier Ahem grep for the same names | unclear — see open question §6 |
| 100 | 4 | `white-space-pre-wrap-trailing-spaces-005/007/008/011` | **HYPOTHESIS**: trailing-space-hang family, likely the same break/collapse-boundary cause, smallest observed diff (a few pixels of hang/overflow). | file-name pattern | new (trailing-space hang) |
| 3637 | 4 | `white-space-normal-011`, `white-space-pre-031/032/035` | **HYPOTHESIS** — `white-space-normal-011` is in the non-Ahem list, so at least that member is unmeasurable; the `pre-0xx` members likely share the break/collapse cause. | file-name pattern; one member confirmed non-Ahem | mixed: TEXTO-blocked (011) + new (pre-03x) |
| 10000 | 4 | `full-width-leading-spaces-002/003/005`, `white-space-intrinsic-size-002` | **HYPOTHESIS**: leading full-width space collapsing (CJK), plus one intrinsic-size member — likely two different causes sharing a pixel count by coincidence (a real risk this triage cannot rule out; see the method's stated limit). | file-name pattern only | new (full-width space) + PCT (intrinsic-size member) |
| 26400 | 4 | `pre-wrap-align-end-002/003`, `pre-wrap-align-start-002/003` | **HYPOTHESIS**: `pre-wrap` combined with `text-align`/logical alignment keywords (`start`/`end`) — could be the break/wrap cause or a `text-align: start/end` resolution bug (LOG lot, "layout runs on logical axes"). Not individually rasterized. | file-name pattern | new (pre-wrap) or LOG — unconfirmed |
| all remaining counts (1120→65000, singles and small groups, ~64 distinct values covering ~55 files) | ~55 | mostly `white-space-intrinsic-size-*` (min-content/max-content sizing under white-space collapsing), a handful of `text-wrap-balance-*`, `tab-*`, single `break-spaces-newline-*` | **MEASURED** for the `white-space-intrinsic-size-020` case (largest single diff in the corpus, 65000px/6.3%): source uses `width: 0` on a `div` to force min-content sizing, with `pre-line` collapsing runs of whitespace, matched against a plain filled-green-square reference. This is squarely an intrinsic-sizing-under-white-space question, not a wrapping-decoy idiom. | `white-space-intrinsic-size-020.html` read | PCT ("a percentagem sabe em que eixo esta" / intrinsic sizing) — PLAN.md §10 names `css-sizing/intrinsic-percent-replaced-*` as its ruler, not this folder, so treat as a NEW instance of the same underlying class rather than a confirmed PCT regression |

**Coverage check**: the two largest rows alone (n=800, n=1250) are 43 of 234
(18%); the `break-spaces`-decoy idiom, taken across every row this triage
attributes to it (n=800, 1250, 625, 1200, 2500, 1600, 400, 5000, 55000, plus
the singles clearly named `break-spaces-*`/`pre-wrap-*`/`pre-line-*` in the
long tail), covers roughly **150 of the 234 failures (≈64%)** — MEASURED for a
sample of ~10 files across the largest groups, HYPOTHESIS by file-name for the
rest. The `textarea` `is_ahem` bug (n=36160 + n=694 + assorted `textarea-*`
singles) covers a further **~20 files (≈9%)**, MEASURED directly against the
Rust source. Together these two causes account for roughly 73% of the
failures — short of, but close to, the ≥70% target this triage set for itself,
without double-counting the 33 non-Ahem files that are simply unmeasurable.

## 3. The 30 `passa-vazio`

All 30 are non-Ahem (grepped, and 3 of them — `white-space-collapse-discard-001`,
`white-space-collapse-preserve-breaks-001`, `white-space-trim-discard-inner-001`
— are actually `.xht` files, not `.html`; also grepped, also non-Ahem). This is
consistent with the hypothesis in the task brief: they pass empty because
neither side paints any visible content once text is masked out — they prove
nothing about layout correctness (issue #2729). This triage did NOT rasterize
any of the 30 individually to check for a border/background difference hiding
under the "both blank" verdict; that is an open gap (§6).

## 4. The 4 `falha-parcial`

- `line-edge-white-space-collapse-001` — non-Ahem; one side paints something
  the other doesn't. Not rasterized; cause unknown.
- `textarea-pre-wrap-014` — Ahem-family name; almost certainly the same
  `layout/input.rs` `is_ahem: false` bug as the rest of the `textarea-*` group,
  but the PARTIAL state (rather than plain `falha`) suggests an additional
  asymmetry — not confirmed by rasterizing this specific file.
- `white-space-intrinsic-size-022`, `-023` — both appear in the non-Ahem grep
  AND in the small `n=528` pixel group as `[PARCIAL]`. Not rasterized
  individually.

None of the 4 were visually inspected in this pass; flagged as unresolved.

## 5. Order of attack

Ranked by files-per-cause and by whether TODAY's Ahem-only raster can measure
progress on it:

1. **`layout/input.rs` `is_ahem: false` (textarea text-painting gap)** — ~20
   files, MEASURED (code-confirmed at two call sites), fully measurable today
   with the existing raster once fixed, no dependency on TEXTO's general glyph
   work. Cheapest, highest-confidence fix in this triage.
2. **`break-spaces`/`pre-wrap`/`pre-line` wrap-opportunity placement in
   `layout/quebra.rs`** — up to ~150 files (MEASURED for a sample, HYPOTHESIS
   for the bulk by file-name), fully measurable today (all Ahem). Highest
   file count by far, but the exact defect (how many lines a break-spaces run
   should occupy) was only pinned down qualitatively here — needs a focused
   look at `quebra.rs`/`segmento.rs`'s per-character break-opportunity logic
   before fixing, and the n=55000 row (largest diffs) suggests there may be a
   SECOND, related bug about collapsing across a `&NewLine;`/sibling-inline
   boundary rather than one uniform cause.
3. **`white-space-intrinsic-size-*` (min-content/max-content under
   white-space collapsing)** — ~15-18 files, MEASURED for one file, maps to
   the PCT lot's unfinished half ("a entidade chegar a
   resolve_height/resolve_inset/substituido.rs vinda da arvore"). Measurable
   today.
4. **The 33 non-Ahem files** — blocked on TEXTO (glyph painting for real
   fonts). Not attackable without that lot landing first; re-triage once it
   does, since several of these (`tab-bidi-001`, `line-edge-white-space-collapse-*`)
   may turn out to be pure layout bugs once text is visible, not font gaps.
5. **The unresolved `n=512` contradiction and the 30 `passa-vazio`** — cheap
   to resolve (a handful of individual rasterizations) but low file count;
   do last.

## 6. What this triage could NOT decide

- **The `ws-break-spaces-applies-to-008/009/010/011/014` contradiction.**
  These five names are BOTH in the `n=512` Ahem-bearing pixel group (implying
  they were classified as using Ahem when I grouped by pixel count) AND in the
  non-Ahem grep list from §1. That means either the grep is right and the
  n=512 group's members are a different, unlisted subset of five files with a
  coincidentally identical pixel count, or the grep has a false negative on
  these five (e.g. Ahem loaded via an `@font-face`/stylesheet this triage's
  regex missed). Not resolved — flagged rather than guessed.
- **Whether pixel-count equality within the large `break-spaces` groups
  (n=625, 1200, 2500, 1600, 400, 5000) really is one cause each**, versus two
  different causes that happen to leak the same-sized decoy strip at a given
  font-size. Only a sample (≈10 of ~150 files in this family) was individually
  rasterized; the method's own stated limit (a count equal to a reference's
  standard box only proves "the whole box is wrong") applies here too, since
  several of these counts (800, 1250, 625, 1600, 2500, 5000) are plausible
  "N rows of an Ahem square at a given font-size" numbers rather than
  evidence of a shared mechanism.
- **The 30 `passa-vazio` files were not individually rasterized**, so "both
  sides paint nothing" is trusted from the report's own classification, not
  re-verified pixel-by-pixel by this triage.
- **The 4 `falha-parcial` causes are unknown** — not one of them was opened.
- **Whether the n=55000 outliers (worst 8 diffs in the whole corpus) share
  ONE cause or two** (a `&NewLine;`-adjacent-inline collapsing bug, and/or a
  simple wrap-opportunity bug at a large font-size) was not settled; only
  `break-spaces-051`'s source was read, none of the 8 were rasterized.
- **No attempt was made to attribute the `eol-spaces-bidi-*`, `tab-bidi-001`,
  or `text-wrap-balance-*` files to a specific cause beyond noting their
  non-Ahem status or grouping by name** — bidi and `text-wrap: balance` are
  each plausible distinct causes this triage did not have budget to open.
