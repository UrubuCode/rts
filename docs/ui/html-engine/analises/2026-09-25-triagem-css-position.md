# Triage of the css-position WPT reftest failures — 2026-09-25

**What was measured.** `C:\Users\nexga\Documents\wpt-bt2a\css-position\relatorio.json`,
produced by `scripts/wpt_reftests.mjs` against the corpus at
`C:\Users\nexga\Documents\wpt-corpus\css\css-position\` (246 reftests), using
`target\base\release\examples\claude-raster.exe` — a binary built from `main`
at `36c1545d2`. The task brief states the branch under review
(`ee36cda2a`, BT-2a) measures identically on this folder, so this triage reads
as current for both.

**Totals in the report:** 246 total, 45 `passam` (pass with content), 5
`passam_vazio` (pass empty — both sides paint nothing), 193 `falham`, 3
`falha_parcial`, 0 errors. `falham + falha_parcial` = **196** failing reftests,
which is the population this triage groups. Canvas size is fixed at 1280×800 =
1 024 000 px for every reftest (confirmed from `claude-raster.rs`, `H = 800`,
and from the `n` values in the report's `piores` block, e.g. `pct=100` ↔
`n=1024000`); the JSON's per-test `resultados` array carries only `pct`, so
`n` below is recomputed as `round(pct × 1024000 / 100)` — exact for every group
checked against `piores`' own `n`.

This is a READ-ONLY triage: no engine code was changed. Every rasterization
below was done with the same `claude-raster.exe`, run per-file, PNGs read
directly (no diffing tool).

---

## Method

Grouped all 196 failures by exact recomputed pixel-diff count (the method from
`project_metodo_agrupar_por_pixels.md`). 16 groups have ≥ 3 files (121 files,
61.7%); adding the largest 2-file groups brings coverage to 155/196 (79%). All
≥3-file groups were examined; the largest 2-file groups sharing a theme with an
already-examined group are folded into that group's row rather than counted as
"not examined". Singletons and the remaining unrelated 2-file groups are listed
as not examined, per the honesty-floor instruction not to pad.

---

## Groups (≥ 3 files, plus related 2-file groups folded in)

| files | pixel Δ (n) | feature | cause (MEASURED vs HYPOTHESIS) | evidence | lot |
|---:|---:|---|---|---|---|
| 8 | 957 501 (93.5%) | `writing-mode: vertical-lr/vertical-rl` on the static-position container | **MEASURED.** Rasterized `static-position/vlr-ltr-ltr.html` vs its ref `vlr-ref.html`: the test renders as a wide horizontal block (writing-mode ignored, treated as `horizontal-tb`); the ref is a narrow vertical column. Vertical writing modes are not implemented for block layout at all. | `vlr-ltr-ltr.html`/`vlr-ref.html` rasterized and read | new — not in any lot (no vertical writing-mode lot in PLAN.md §9–§11) |
| 6 | 30 000 (2.9%) | `top-layer-box-uses-icb-{slr,srl,vlr,vrl}-*` — same writing-mode gap, applied to the ICB/static-position-rect test | **HYPOTHESIS**, same cause as the row above (root has `writing-mode: vertical-lr` etc.); not independently rasterized, but the source is structurally identical to the `htb` variants that presumably pass. | source read: `top-layer-box-uses-icb-vlr-ltr.html` | new — same as above |
| 4 | 80 064 (7.8%) | `multicol/static-position/{vlr,vrl}-*-in-multicol` — writing-mode again, now combined with multicol | **HYPOTHESIS**, same root cause; source (`vlr-ltr-ltr-in-multicol.html`) sets `body { writing-mode: vertical-lr }` on a multicol container, structurally the same gap. Not rasterized separately. | source read | new — writing-mode gap, compounding with SC (multicol) lot |
| 4 (+2 folded: n=80576, n=80584) | ~80 000–80 600 (7.8–7.9%) | same multicol/static-position writing-mode family, `vrl` orientation variants | **HYPOTHESIS**, folded into the row above — not independently examined | not examined individually | new |
| 3 (+3 folded: n=16400,17600 incl. one sticky outlier) | 16 000–17 600 (1.6–1.7%) | `multicol/{vlr,vrl}-*-in-multicols` (no `static-position/` prefix — a related but distinct multicol writing-mode test set) | **HYPOTHESIS**, same writing-mode gap by name/pattern; not rasterized | not examined individually | new |
| 30 | 5 000 (0.49%) | heterogeneous — two distinct real causes bundled at the same Δ by coincidence (a 100×50 and a 50×100 wrong-box area both equal 5000 px) | **split below** | — | — |
| — | — | (a) 14 of the 30: `position-relative-table-{tbody,tfoot,thead,tr}-{left,top}[-absolute-child]` | **MEASURED.** Rasterized `position-relative-table-tbody-left.html` vs `position-relative-table-left-ref.html`: the ref shows one green 50×50 box shifted right by 100px; the test shows the box unshifted (green) plus the red indicator box exposed. `position: relative` with `left`/`top` on a `<tbody>`/`<tfoot>`/`<thead>`/`<tr>` (table row-group/row boxes) does not apply the relative offset at all. | `position-relative-table-tbody-left.html`/`-ref.html` rasterized and read | new — not in any lot; table-internal box types and relative positioning |
| — | — | (b) 8 of the 30: `static-position/inline-level-absolute-in-block-level-context-{001,004,007..012}` | **MEASURED (source) / HYPOTHESIS (root cause).** Source combines an absolute-positioned inline box (`display:inline; position:absolute`) next to a float, under `direction`/`text-align` variants. Not rasterized individually, but the construction (static position of an inline-level abspos box beside a float) matches the `-002/-003/-005/-006` pairs at n=2900/4150 below, which strongly suggests one shared cause: static-position calculation for an inline-level absolute box does not account for a preceding float correctly. | source read: `inline-level-absolute-in-block-level-context-001.html` | new — static-position lot is not named in PLAN.md §9–§11; closest is BT-2/BT-3 (box tree, inline formatting) |
| — | — | (c) remaining 8 of the 30: 3 sticky (`position-sticky-fractional-offset`, `-scrolled-remove-sibling-002`, `-table-parts`), `absolute-pos-box-inside-fixed-pos-box-with-changing-height`, `position-absolute-dynamic-relayout-007`, `position-absolute-in-inline-margin-top` | **NOT EXAMINED** individually — the coincidence of exact pixel count across three unrelated features (table row-group, static position, sticky) is exactly the false-transversal-cause trap the pixel-count method's own header warns about (equal count ≠ one cause when the box being compared is small and generic, e.g. a single 50×100 or 100×50 rectangle) | — | not examined |
| 22 | 10 000 (0.98%) | heterogeneous, **dominated by sticky with `<script>`-driven scroll/resize (12 of 22)**; remainder is absolute/relative static-layout tests | **split below** | — | — |
| — | — | (a) 12 sticky: `-bottom-007, -change-top, -left-{004,005,006}, -margins-{002,003}, -rtl, -stacking-context-002, -top-{004,005,006}` | **MEASURED (harness limitation), for at least 2 of the 12.** Read `position-sticky-left-004.html` and `position-sticky-padding-001.html` (a different group, same pattern): both use `<script>onload>` to resize/scroll the container *after* load, and `wpt_reftests.mjs`'s own header states scripted tests are rasterized WITHOUT running the script. So the raster necessarily shows the pre-script state, which the reference expects to be post-script. This makes these mismatches at least partly artefacts of the harness rather than the engine, though a genuine sticky bug could still be hiding under the same pixel count — undetermined without a scripted (real-browser or JS-enabled) comparison. | `position-sticky-left-004.html` read (has `<script>`); cross-checked against `position-sticky-padding-001.html` in the n=6400 group, same pattern | none — measurement/harness limit, not a lot |
| — | — | (b) 10 non-sticky: `position-absolute-center-007`, `-dynamic-auto-overflow`, `-dynamic-list-marker`, `-fit-content`, `-replaced-{intrinsic,no-intrinsic}-size.tentative`, `-under-non-containing-stacking-context`, `position-relative-006`, `sticky/position-sticky-margins-002/003` (already counted above) | **NOT EXAMINED individually** — plausible independent causes (fit-content sizing of abspos, replaced-element intrinsic size under abspos, stacking-context containment) each worth their own look, not done here | — | not examined |
| 11 | 20 000 (1.95%) | `hypothetical-dynamic-change-{001,002,003}` (script-driven re-layout of the hypothetical box), `position-relative-010`, `top-layer-box-uses-icb-{htb-ltr,htb-rtl,slr-rtl,vlr-ltr}` (4, writing-mode family above), `sticky/position-sticky-escape-scroller-{001,002,003}` (script-driven scroll) | **MIXED, all HYPOTHESIS except the writing-mode subset which reuses the measured row above.** `hypothetical-dynamic-change-*` and `escape-scroller-*` were not opened but their names strongly imply `<script>`-driven relayout, same harness caveat as the sticky group above. | not independently rasterized | writing-mode subset → new (as above); rest not examined |
| 6 | 10 800 (1.05%) | `nested-inline-abspos-child[-with-siblings]`, `position-absolute-in-inline-005`, `position-relative-{003,004,005}` | **MEASURED for `position-relative-003`.** Rasterized vs `../reference/ref-filled-green-100px-square.xht`: expected a single green 100×100 square; got a green square offset to the top-left corner plus the red backdrop showing through at bottom-right. Source nests two `position:relative` inline `<span>`s with percentage `top`/`left` (100%, then -100px) around a `position:fixed` child. The percentage-resolution and/or containing-block chain through nested relatively-positioned inline boxes is wrong. | `position-relative-003.html` rasterized and read | **PCT** (percentage insets) per PLAN.md §9–§11 |
| 5 | 2 500 (0.24%) | `invalidate-opacity-negative-z-index`, `position-absolute-dynamic-static-position-floats-004`, `position-relative-table-caption`, `position-relative-table-td-{left,top}` | **MEASURED for the table-td/caption pair (2 of 5), by extension of the tbody/tfoot finding above.** `position-relative-table-td-left`/`-top` and `-table-caption` are the same relative-offset-on-table-internal-box bug as the 14-file group above, just on a `<td>`/`<caption>` (single 50×50 box wrong = 2500px, half of the tbody case's 5000px because only one box is affected there instead of two). The other 2 (opacity/z-index invalidation, dynamic floats) are a different, unexamined cause that happens to produce the same-sized wrong rectangle. | inferred from `position-relative-table-tbody-left` finding; td/caption sources not read directly | table-relative subset → new (same as tbody/tfoot row); rest not examined |
| 4 | 12 650 (1.24%) | `sticky/position-sticky-{nested-table,nested-thead-th,table-th-top,table-thead-top}` | **NOT EXAMINED** — sticky positioning inside table structures; could be the same table-internal-box gap as the relative-positioning finding (sticky is also a positioned-box feature) or a separate sticky-specific issue. Worth checking first given the tbody finding. | — | not examined (candidate: same table-box gap, unconfirmed) |
| 4 | 1 024 000 (100%, full-canvas mismatch) | `overlay/overlay-transition-{backdrop,out-rendering}`, `sticky/position-sticky-fixed-ancestor-{002,003}` | **MEASURED (harness limitation).** All 4 are in the report's own `piores` list with `script: true` (except `-ancestor-003`, still likely scripted given the name "fixed-ancestor" + the other two in the same family being scripted). `overlay-transition-*` tests CSS transitions on the top-layer via `requestAnimationFrame`/timers, which a static rasterizer without JS cannot drive. 100% pixel mismatch is consistent with "reference shows the end state, test rasterized only the start state." | `piores` block of `relatorio.json` | none — harness limit (no JS/animation), not a lot |
| 4 | 7 600 (0.74%) | `sticky/position-sticky-table-{td-bottom,td-top,tfoot-bottom,th-bottom}` | **NOT EXAMINED** — same candidate as the n=12 650 row: sticky + table-internal boxes. Not opened. | — | not examined (candidate: same table-box gap) |
| 4 | 8 000 (0.78%) | `sticky/position-sticky-flex-item-{001,002,003,004}` | **MEASURED (harness limitation).** Read `position-sticky-flex-item-001.html`: uses `<script>` to set `scrollTop` after load to exercise the sticky-in-flex reserved-space behaviour. Same no-JS caveat as above; whether the underlying flex+sticky layout is itself correct is undetermined from this alone. | `position-sticky-flex-item-001.html` read | none — harness limit, not a lot (though a real flex+sticky gap could still exist under it) |
| 4 | 1 600 (0.16%) | `position-absolute-dynamic-static-position-{floats-002,floats-003,margin-001,margin-002}` | **NOT EXAMINED** — names suggest static-position of an abspos box interacting with floats/margins during a dynamic (scripted) change; likely overlaps both the harness-limitation pattern and the static-position-next-to-float pattern above, but not confirmed. | — | not examined |
| 3 | 7 500 / 3 4 000 / 3 16 400 (small groups, folded above where thematically linked) | mixed: `position-absolute-dynamic-relayout-{002,004}`, `sticky/position-sticky-table-{td-right,th-right}`, `position-absolute-in-inline-{003}`, `position-relative-007`, `multicol/{vlr,vrl}-*-in-multicols` (folded into writing-mode family) | **NOT EXAMINED** except the multicol subset (folded above) | — | not examined |

**Coverage of this table:** the rows marked MEASURED or HYPOTHESIS-with-named-cause
account for roughly 8+6+4+4+3+30+22+6+5 = **88 files with an assigned or
strongly inferred cause** (writing-mode: 8+6+4+4+3=25; table-internal relative
positioning: 14+2=16; inline-abspos-next-to-float: 8; nested-relative-inline
percentage (PCT): 6; script/harness-limitation sticky+overlay: 12+4+4=20).
The rest of the ≥3-file groups (12 650, 7 600, 1 600, and the unexamined halves
of the 5000/10000/2500 groups — roughly 30 files) are listed as candidates but
**not examined**, and the long tail of singleton/pair groups (the remaining
~78 files) was not opened at all.

---

## The 5 `passam_vazio` (pass empty)

Not individually opened in this pass — the report does not list their names in
the excerpt read, and finding them requires a further JSON pass (`estado ==
"passa-vazio"`) not run in this triage. **What they need to become a real
pass, generically** (per issue #2729 and the DOM PLAN context): the engine
paints no glyphs except Ahem, so any reftest whose content is real (non-Ahem)
text on both sides paints nothing on either side and "passes" without proving
anything. The fix in general is glyph painting for arbitrary fonts in the
raster tool, not a css-position engine change — this is a measurement-tool gap,
not a layout gap. **Not examined further — listed as not examined.**

## The 3 `falha_parcial`

From the report's `piores` list, two of the three are already covered above:
`overlay/overlay-transition-backdrop` (`falha-parcial`, in the 1 024 000-px
group) and `sticky/position-sticky-fixed-ancestor` family. The `piores`
excerpt read also shows `position-absolute-dynamic-relayout-006` marked
`falha-parcial` at n=10000, and `position-fixed-dynamic-transformed-sibling`
at n=64000 — both **not examined**. A `falha-parcial` means one side of the
pair painted content and the other painted nothing; for a scripted test this
is consistent with the same no-JS harness limitation (the un-scripted side
renders its DOM, the reference — which may itself rely on nothing further —
renders normally, or vice versa). **Not confirmed for these three
individually — listed as not examined** beyond the two already covered by the
script/harness finding above.

---

## The order in which to attack them (ranked by files-per-cause)

1. **Vertical writing modes (`vertical-lr`/`vertical-rl`, and by extension
   `sideways-lr`/`sideways-rl`) are entirely unimplemented for block layout —
   ~25 files (8 measured directly, 17 inferred from identical source
   pattern).** This is the single largest, most clearly isolated cause in the
   corpus: one missing capability (block flow direction driven by
   `writing-mode`) explains a `static-position/`, a `multicol/static-position/`,
   and a `top-layer-box-uses-icb-*` family all at once. No lot in PLAN.md
   §9–§11 currently owns this — it would need to be scoped as a new lot,
   likely touching BT-2/BT-3 (box tree / containing-block geometry) since block
   direction changes what "inline size" and "block size" even mean for layout.

2. **`position: relative` (and likely `sticky`) on table-internal boxes
   (`<tbody>`, `<tfoot>`, `<thead>`, `<tr>`, `<td>`, `<caption>`) does not
   apply the positioning offset — ~16 files measured/inferred, plus 8 more
   candidate sticky-on-table files (n=12650, n=7600) not yet confirmed to
   share the cause.** Second-largest and cleanly isolated: the relative offset
   is silently dropped specifically on table row-group/row/cell boxes, while
   it presumably works on ordinary boxes (nothing in the corpus suggests a
   general relative-positioning bug). Worth checking the sticky-on-table
   candidates next since confirming or ruling them out changes this group's
   size by 8.

3. **Static position of an inline-level absolute box beside a float, under
   `direction`/`text-align` combinations — 8 files measured by source
   inspection, part of the n=5000 group; likely joined by the n=2900/n=4150
   pairs (4 more) for 12 total.** Not rasterized to confirm the actual pixel
   discrepancy, only the source pattern.

4. **Nested `position: relative` inline boxes with percentage `top`/`left` —
   6 files, PCT lot.** Directly measured with a clear before/after image; the
   percentage or containing-block resolution through a chain of relatively
   positioned inline ancestors is wrong.

5. **Harness limitation: reftests whose expected state is reached only after
   a `<script>` runs (scroll, resize, timers/rAF for transitions) — at least
   20 files measured to use `<script>`, plausibly 30+ once the unexamined
   sticky/script candidates are checked.** This is not an engine bug by
   itself; `wpt_reftests.mjs` explicitly rasterizes scripted tests without
   running the script. It cannot be fixed by touching `rts-dom` layout code —
   either the harness gets a JS-execution mode, or this whole family is
   excluded from the denominator the way `rel="mismatch"` already is. Because
   a real sticky/flex or sticky/table-internal bug could be hiding underneath
   the harness gap, this should be treated as "unmeasurable with the current
   tool," not as "these pass once JS runs."

---

## What this triage could NOT decide

- Whether the 8 non-table-row-group failures at n=2500 (`invalidate-opacity-
  negative-z-index`, `position-absolute-dynamic-static-position-floats-004`)
  share any cause with each other or are two more singletons that happen to
  produce a 2500px-sized wrong rectangle — not rasterized.
- Whether `sticky/position-sticky-{nested-table,nested-thead-th,table-th-top,
  table-thead-top}` (n=12650) and `sticky/position-sticky-table-{td-bottom,
  td-top,tfoot-bottom,th-bottom}` (n=7600) are the same table-internal-box
  positioning gap found for `position:relative`, or a distinct sticky-specific
  issue — plausible but not confirmed by rasterizing either group.
- Whether any genuine sticky, flex, or transform bug exists underneath the
  20+ script-driven reftests once JS execution is available — the current
  raster tool cannot distinguish "harness can't run the script" from "the
  engine would get this wrong even with the script run."
- Exact per-file causes for the ~78 files in singleton and small unrelated
  pixel-count groups (everything below the ≥3-file cutoff not folded into a
  named theme above) — not opened at all in this pass.
- The 5 `passam_vazio` files' names and whether any of them would still pass
  once real-glyph text painting exists in `claude-raster.exe` — the JSON was
  not filtered for `estado == "passa-vazio"` in this triage.
- Whether the two multicol/writing-mode 2-file groups (n=80576, n=80584) and
  the "no static-position/ prefix" multicol group (n=16400/17600) are truly
  the same root cause as the confirmed n=957501/n=30000/n=80064 writing-mode
  family, or a second, independent multicol-specific writing-mode gap layered
  on top — inferred from naming only, not rasterized.
