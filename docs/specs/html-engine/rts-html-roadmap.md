# RTS HTML render engine — OPERATIONAL ROADMAP (F0-F5)

> ## ⚠️⚠️ REVERSAL OF DECISION #2 (2026-06-27) — LAYOUT MOVES TO rts-dom
> **Developer decision (Marcos), 2026-06-27:** *"process everything in the DOM and
> egui only reads and displays"*. This **REVERTS the central decision of this roadmap**
> (point #2 of the §2 table: "egui does layout by default"). Reason: with layout in egui
> **"it will be impossible to swap the UI"** — position computation stays locked to the
> backend and the headless DOM stays incomplete (it knows style, not POSITION).
>
> **New official direction = the 5 trees of [`rts-html-north-star.md`](rts-html-north-star.md)**
> (which STOPS being "frozen/does-not-dictate-phases" and becomes the target architecture
> again): `rts-dom` computes DOM→Style→**LAYOUT (x,y,w,h)**→DisplayList; `rts-egui` **only
> paints** the display-list (ready Rect/Galley) + serves text measurement via the
> `TextMeasurer` trait. **egui never decides layout again.**
>
> The F0–F5 phases below (egui-layout in-place) **remain as a historical record of
> what was delivered** (F0/F1/F2 + `<style>` tag — the STYLE/cascade in rts-dom is
> already right and is reused). NEW work follows the north-star's P0–P7 pipeline
> (own layout, pixel-first). The current `render.rs` (ui.label/
> horizontal/Frame) is LEGACY to be replaced and deleted once the new covers the old.
> See memory `project_layout_moves_to_dom`.

> **This is the living execution plan of the RTS HTML render engine.** It is the only
> source of sliced work. The [`rts-html-north-star.md`](rts-html-north-star.md)
> (the old 5-tree `PLANO.md`) ~~is frozen conceptual reference and does NOT
> dictate phases~~ **← is BACK to being the target direction (see reversal above, 2026-06-27)**.
>
> Decision made on 2026-06-23 after multi-agent analysis (4 approaches × 3 adversarial
> lenses — TS-engine feasibility, doctrine, cost/risk — + completeness critique).
> Code language: Rust (English). Communication: Portuguese.

---

## 1) The strategy in one sentence

**Evolve the light engine already on main (the "approach B": retained DOM in an arena +
data-driven block allocator in TS + mutation by NodeId, all in
`crates/rts-egui/`) IN-PLACE, in usable stages. egui is the layout and text-measurement
engine BY DEFAULT and forever in the common cases. CSS (color / box /
cascade) and events arrive early via ABI extensions with OPAQUE NUMERIC SLOT +
batched-cascade-in-Rust. Absolute paint (`allocate_painter` +
`LayoutJob`/`galley.rows`) enters as a SURGICAL EXCEPTION in a single `render_*` ONLY at the
point where egui provably does not compose (rich inline paragraph with
link/hit-test). NEVER create `rts-html`, recreate `dom.rs`/tokenizer/ABI, nor
persist a string handle or NodeId across frames.**

Why not the north-star's 5 trees: rewriting B into a crate would destroy 24
headless tests, the proven ABI and the shared-GPU fix, in exchange for architectural
purity that egui makes unnecessary. The north-star §3 rule "universal absolute paint"
costs 40-60% of the effort (the north-star's own Risk 1) without moving
the ergonomics for the TS developer.

---

## 2) The 10 decision points, resolved

| # | Point | Decision |
|---|---|---|
| 1 | Official light path OR 5 trees? | **Light (B) is official and permanent.** The 5 trees are never born as a global pipeline; the maximum is a mini-paragraph-layout inside ONE `render_*` (F4). |
| 2 | Layout in egui or own engine? *(the central contradiction)* | **egui does the layout by default, always, in the 4 displays; absolute paint is a scoped exception (F4).** Conscious heterogeneity: each display picks its engine. |
| 3 | Create `rts-html` or stay in `rts-egui`? | **100% in `rts-egui`.** `dom.rs`/`block.rs`/`html.rs`/`style.rs` are already egui-free and headless-testable inside the crate. The cross-crate `TextMeasurer` inversion is ceremony only universal paint would justify. |
| 4 | Text-measurement boundary | **egui's `LayoutJob`/`galley.rows`, always.** Run-by-run `glyph_width` (a door the north-star itself closed, Risk 1) is NEVER implemented. |
| 5 | Style: `BlockDef` or `ComputedStyle`+cascade? | **Additive evolution.** `BlockDef` remains as the *display* UA-stylesheet; a `ComputedStyle` per NodeId is born, computed by cascade IN RUST (tag-default < `.class` < `#id` < inline `style`), fed by opaque numeric slots. |
| 6 | Events/hit-testing OR only mutation? | **Both.** Mutation by NodeId stays (an asset of B the north-star didn't even foresee). Events enter via **polling with a mechanism-agnostic contract** (`pollEvent(h) → NodeId i64`, sentinel `-1`). No reactive listener (blocked by #195). |
| 7 | Ergonomic facade given the engine limits | **Raw contract by NodeId is the stable base; ergonomics is a `.ts` lib injected via prelude into the flattened program (`CONSOLE_TS` pattern), NOT imported from another module.** No capturing callback; no chaining without annotation; string getter becomes `getText(node) → Handle` re-read at each use. |
| 8 | Cache/incrementality | **Defer until MEASURED; owner = `UiCtx`.** `ComputedStyle` in a `Vec` parallel to the arena with a coarse dirty-flag; absolute galleys cached only in the E-painter, invalidated by hash(text+style+width)+**DPI**. No tree-Arc (north-star Risk 6). |
| 9 | CSS/HTML subset and cuts | **Inherit all the north-star's permanent cuts** (flexbox/CSS-grid/position/z-index/transform/var()/reactive-:hover/RTL/font-family). Incremental IN: color/bg/font-size → margin/padding/border/width% → rich inline+links. |
| 10 | Concrete reuse | **Rewrite ONLY internal functions of `frame.rs`.** `dom.rs`/`block.rs`/`html.rs`/ABI/tokenizer/`present()`/`SharedGpu` remain intact. |

---

## 3) The 6 hard invariants

These are not footnotes — they are acceptance conditions for any engine PR.
They came out of the completeness critique (gaps NO original approach covered).

1. **String handle NEVER persisted across frames.** The `run()` loop calls
   `getText` per frame; a Handle stored without a stack root is collected in
   `finish_cycle()` (every 256 allocs, mark+sweep GC). `getText → Handle` is
   always re-read, never cached in TS. The source of truth for text is the Rust
   arena (no TS mirror — this also eliminates desync).
2. **Versioned NodeId `{idx, gen}`.** Without a generation, a NodeId recycled after a
   re-parse applies state to the wrong live node (a **memory-safety** bug).
   Every structure indexed by NodeId validates `gen`. Prerequisite of F2/F3/F5.
3. **Sentinel `i64 = -1`, never `u64::MAX`.** `0xFFFF_FFFF_FFFF_FFFF` > 2^53 is not
   exact as `number` and the inline comparison goes wrong. All optional-NodeId
   returns use `-1` + the rule "extract the return into a const before
   comparing" (see [[project_codegen_i64_cmp_bug]]). Standardizes `query*`,
   `pollEvent`, `createElement`.
4. **Opaque numeric slot for CSS.** Rust NEVER matches on a CSS string
   (`"background-color"`). `defineStyle`/`setStyle`/`setStyleBatch` receive an
   index; TS maps CSS-name → index (just like `display = 0..3` in `block.rs`).
   Review criterion: adding `box-shadow` requires only registering a slot in TS,
   never touching `style.rs`. (Doctrine: the front never names non-native vocabulary.)
5. **Flattened facade via `.ts` prelude, never an import from another module.** `new` of a
   class imported from another module bails in the new engine (see
   [[project_new_engine_dispatch_limits]]); the `Element`/`Document` facade is
   injected into the flattened program (`CONSOLE_TS` pattern), and the API is top-level
   functions receiving a handle, never `query().setText()` chaining without annotation.
6. **BATCH form mandatory for style.** Styling 1 node is 5+ props;
   cascading over N nodes would be N×5 FFIs/frame, and the engine is already ~6× slow in
   array-heavy workloads (see [[project_array_perf_and_int32]]). `setStyleBatch(h,
   buffer_handle)` with `(nodeId, slot, val)[]` since F2.

---

## 4) Phased roadmap

Ordered by **value-per-effort and pixel-early**. Each phase delivers a usable window.
The central contradiction (egui-layout × absolute paint) is faced ONLY in **F4** —
the first point where egui *provably* does not compose; F0-F3 already deliver the
safety net (if F4 slips, nothing regresses).

### F0 — Safety foundation (zero new pixels). PREREQUISITE OF EVERYTHING. — ✅ DONE (2026-06-24)
- **Usable:** everything that runs today keeps running + sound base for caches/events.
- **Delivers:** (a) **version NodeId** `{idx, gen}` (invariant 2); (b) hash of the
  HTML string in the `UiCtx`; (c) **split of `frame.rs`** (already > 500 lines — the
  `read_before_commit.sh` gate fires) into `frame/render_block.rs` /
  `render_inline.rs` / `painter.rs`; (d) egui-free `style.rs` with OWN types
  (`u32 RGBA`, `Dimension{Auto,Px,Percent}`) — **never** `Color32`/`FontId`/`Vec2`
  (otherwise the anti-`rts-html` argument falls and the separation becomes a lie); (e) **3
  proof fixtures** (`claude-egui-*`): invoke-of-fn-handle-from-Map (prove whether it
  bails), `new Window` via flattened prelude compiles, `getText→Handle→gc-read`
  survives `finish_cycle()`.
- **Reuses:** everything; only adds fields. **Abandons:** NodeId-without-generation;
  monolithic `frame.rs`. **Effort:** low-medium (~1 wk).
- **Gate/risk:** if the fn-handle-from-Map fixture bails (likely — partial `funcval`,
  collections lose type), the F3 event model is already born without storing a function.

> **STATUS — ✅ DELIVERED.** (a) Versioned NodeId `{gen,idx}`, validated by `gen`,
> packed `(gen<<32)|idx` into i64 at the ABI (PR #1730). (b) `UiCtx.html_hash` — only
> re-parses if the string changes; identical preserves tree+generation (PR #1733). (c)
> `frame.rs` 835→`frame/{gpu,mod,render}.rs` all <500 (PR #1731). (d) egui-free
> `style.rs`: `u32 RGBA` (`type Rgba`) + `ComputedStyle` + OPAQUE `apply_slot(slot,val)`
> (SLOT_COLOR=0/BG=1/FONT_SIZE=2); `u32→Color32` conversion in the render (PR
> #1732). **Off-plan bonus:** sentinel `-1` (inv. 3) + HTML entities (PR
> #1728); vsync fix `PresentMode::Fifo` + `vsync_kill_gate` — the window would stall
> on its own without vsync (PR #1729/#1731). 32 green tests. **Deviations from the plan:** (c)
> the actual split ended up `gpu/mod/render` (not `render_block/render_inline/painter`
> — grouped by responsibility GPU vs cycle vs render); (d) `Dimension` did
> NOT enter yet (only color/bg/font_size — `Dimension{Auto,Px,Percent}` arrives in F2 with the
> box model). (e) the 3 proof fixtures were left PENDING: they depend on `getText`
> and the flattened prelude, which do not exist yet — moved to the start of F3
> (events/facade), where they are a natural prerequisite.

> **✅ F0 FOLLOW-UP — REANALYSIS DONE (2026-06-25).** Audit (doctrine +
> efficiency) concluded. **Doctrine/invariants: 100% COMPLIANT** — no feature
> strayed from the pattern; no CSS string match outside `parse_inline`, `style.rs`
> egui-free in the code, `NodeIdx` doesn't leak to the ABI, sentinel `-1`, ABI with correct
> types. **Efficiency: clean hot path**; 2 per-frame allocations cut (PR
> #1736): `html()` no longer allocates the string on the no-op path (hash over `&str`), and
> `render_block` no longer clones the tag (borrowed `as_str`). **Minor pending follow-up
> (P2/P3, not urgent):** `set_text`/`set_attr` with double `&str→String` allocation;
> `parse_inline` of `style=""` re-parsed per node without cache. The only pattern "deviation"
> is the `gpu/mod/render` split (already documented below) — a decision, not a
> violation. Original follow-up text below (kept for reference):
>
> **⚠️ F0 FOLLOW-UP — EFFICIENCY + PATTERN REANALYSIS (pending, do before/during F1).**
> F0 was delivered fast, in 6 sequential PRs; it is possible some feature
> **strayed from the requested pattern** or left inefficiency. Before stacking
> F1 on top, critically review what went in:
> - **Adherence to doctrine/invariants:** is `style.rs` really 100% egui-free in the
>   code (yes, the `egui_free_garantia` test covers it)? did any non-primordial/CSS name
>   leak into Rust (invariant 4)? did the internal `NodeIdx` × boundary versioned `NodeId`
>   separation stay consistent at ALL call sites (widgets/
>   render), or is there a dubious conversion left?
> - **Efficiency:** `html_hash` runs `DefaultHasher` over the whole string per
>   frame — ok for now, but measure whether it becomes a bottleneck on large HTML; is the
>   zero-re-parse actually saving (no other hidden per-frame rebuild)? did
>   `present_mode=Fifo` solve the CPU-spin, but confirm there is no more redundant
>   work in `endFrame`.
> - **Code pattern:** the `frame/` split grouped by responsibility (gpu/mod/
>   render) instead of the `render_block/render_inline/painter` the plan asked for —
>   decide whether that becomes the official pattern (update the plan) or realign.
> - **Output:** either a cleanup/realignment PR, or an explicit note "reviewed,
>   compliant". Do not leave silent debt before F1.

### F1 — Text style (color / font-size / bg) via egui. ⭐ HIGHEST VALUE-PER-EFFORT. — ✅ DONE (2026-06-25)

> **STATUS — ✅ DELIVERED (PR #1738).** ABI `defineStyle(tag, slot, val)` wiring the
> `apply_slot` (F0d) to rendering: thread_local `STYLES` map (tag→ComputedStyle)
> in `style.rs` with `define_style`/`lookup_style` (accumulates slots per tag); the render
> applies in the CSS order inherited < TAG-style < inline `style=""` (`merge_node_style`
> + `apply_computed`); heading combines both. Example `claude-egui-style.ts` (h1
> blue size 28 + gray p via RichText, zero painter) — §7 criterion met,
> visually confirmed. 33 tests. **Did NOT enter (left for later):** (3) `setStyle(h,
> node, slot, val)` PER-NODE — only per-TAG style for now; `bg` (SLOT_BG=1) is
> accepted but not yet painted inline (arrives in the F2 box model via `egui::Frame`).

- **Usable:** colored doc, arbitrary font-size, per-block bg — 100% via egui
  (`RichText.color/.size/.background_color`). Demo: `egui_dom_mutacao.ts`
  styled.
- **Delivers:** `defineStyle(sel, slot:i64, val:i64)` + `setStyle(h, node, slot,
  val)` (opaque slots, invariant 4); **string→value converter in `style.rs`**
  delivered RIGHT here (prerequisite of F1/F2/F3); application only reads the
  `Vec<ComputedStyle>`.
- **Reuses:** `block.rs` (defaults), the `RichText` B already emits; only
  `render_inline`/`render_block_body` consult `ComputedStyle`. **Abandons:**
  ignored `style` attribute; `indent` carrying heading size.
- **Effort:** low-medium (~1.5 wk). **Gate/risk:** no absolute pixel.

### F2 — Block box model (margin / padding / border / bg / width%) via `egui::Frame`. — ✅ DONE (2026-06-27)

> **STATUS — ✅ DELIVERED (branch feat/dom-f2-box-model, 2 commits).** Most of
> the box model (`egui::Frame` with padding→inner_margin/margin→outer_margin/bg→fill/
> border→stroke/radius→corner_radius) had already come ahead from F0/F1; F2 completed:
> (a) full **`Dimension`** in `rts-dom/style.rs` (egui-free): `{Auto, Px,
> Percent, Em, Rem, Vw, Vh}` — ALL the usual length units, not just `%`.
> Each resolves LATE in the render (`resolve(ctx)` against its axis: %=PARENT's
> content-box via `ui.available_width`, em=node font, rem=root font, vw/vh=viewport).
> ABI encoding by RANGES (`DIM_RANGE=1bi`, value×1000, reversible). `parse_inline`
> reads px/%/em/rem/vw/vh/auto + padding/margin/border-*/radius. Render applies via
> `set_max_width`. `SLOT_WIDTH=8`. (b) **`setStyleBatch`** (invariant 6):
> `setStyle(dom,node,slot,val)` + `setStyleBatch(dom,buffer,count)` (triples
> (nodeId,slot,val) i64-LE from an `Entry::Buffer`, read via `rts_engine::heap::handles`
> — no rts-shared dep). Per-node override = 3rd style source (tag<inline<per-node),
> merged in `computed_style`+render (box+text). 51 green tests. Examples
> claude-egui-box-model.ts (units) + claude-dom-setstyle.ts (override/batch,
> headless validated). **Off-plan bonus (DOM/MDN conformance):** navigation
> (parentNode/first|lastChild/next|previousSibling), childNodes, createTextNode,
> insertBefore, nodeType/nodeName, NodeKind::Comment + parser preserves comments,
> classList — brings rts-dom closer to Mozilla's definition (it was "DOM-inspired";
> now faithful to the paradigm with much more coverage). **Engine limit confirmed:**
> `el.setStyle()` on `array[i]` of a class bails (non-dispatchable receiver); the
> examples use the direct `dom.*` primitives in the loop.
> **Pending for an F2.1 (not urgent):** VISUAL validation on screen (screenshots
> captured the wrong window; unit+headless tests cover the logic); comment parser
> preserved but createComment not yet exposed on the ABI.

> **✅ EXTRA (2026-06-27) — `<style>` TAG (declarative author CSS).** Cross-cutting
> F1/F2 feature: the engine now parses `<style>…</style>` and styles via pure CSS,
> without imperative `defineStyle`. Delivered:
> - **Tokenizer** (`html.rs`): `<style>`/`<script>` become `Token::RawElement` —
>   RAW content (CSS/JS is not HTML; `{`, `>` in `a>b`, `<` don't tokenize as a tag),
>   read until `</tag>` case-insensitive.
> - **Stylesheet** (`style.rs`): egui-free `Stylesheet`/`Rule`/`Selector`.
>   Selectors `tag`/`.class`/`#id`/`*`; `parse_rules` reuses `parse_inline` for the
>   `{…}` body; `/* */` comments and selector-list `a,b,.c{}` supported.
> - **Cascade FAITHFUL TO MDN** (`Dom::computed_style`, validated against
>   developer.mozilla.org/Web/CSS/Guides/Cascade): stage 1 origin/importance
>   (UA `defineStyle` < author `<style>` < inline `style=""` < per-node override;
>   then the `!important` on top — **`!important` SUPPORTED**, normal/
>   important layers separated in `DeclBlock`); stage 2 specificity (id=100>class=10>
>   tag=1>universal=0); stage 3 source order (later rule breaks ties);
>   stage 4 inheritance (color/font-size flow down in the render).
> - **Render** (`render.rs`): skips `<style>`/`<script>` (doesn't draw the raw text);
>   uses `computed_style_idx` (full cascade, includes the `<style>` layer).
> - 62 green rts-dom tests; example `claude-dom-style-tag.ts` (headless) proves
>   id>class>tag + inheritance + `!important`.
> - **Conscious cuts** (CSS subset): `@layer`, compound selectors (`.a.b`)/
>   combinators (`div p`, `>`), pseudo-classes (`:hover`), keywords
>   `inherit`/`initial`/`unset`/`revert`.

Original plan text (reference):
- **Usable:** cards/boxes with background, border, radius and spacing; `width%`
  resolved **late** against the parent's content-box (avoids north-star Risk 5).
- **Delivers:** `ComputedStyle` gains `Dimension`; `egui::Frame{inner_margin,
  outer_margin, fill, stroke, corner_radius}` + `set_max_width`. **`setStyleBatch`
  mandatory** from here on (invariant 6).
- **Gate/risk:** declaring `egui::Frame ≠ box model` (no margin-collapse, no
  box-sizing) as a product limit, not a bug.

### F3 — Events via polling (click/hover) with a mechanism-agnostic contract.
- **Usable:** clickable `<a>`/`<button>`; TS loop dispatches by NodeId.
- **Delivers:** **contract defined BEFORE implementing:** `pollEvent(h) →
  (NodeId i64 = -1 if none, optional local coord)`. F3 uses
  `ui.interact`/`Response.clicked()` (egui does the hit-test), but the contract already
  anticipates the F4 painter path — avoids rewriting `pollEvent` later. Handlers
  **don't store fns**: `pollEvent` + switch-by-NodeId in the loop, state in a
  module-level gcell (works around #195 and the invoke-of-fn-from-Map that may bail).
- **Reuses:** `button_results`/cursor pattern from `widgets.rs`;
  `id_index`/versioned NodeId from F0. **Abandons:** capturing `onClick(()=>count++)`
  (bails; showcase example rewritten to gcell).
- **Effort:** medium (~2-3 wk). **Gate/risk:** 1-frame latency (known ceiling,
  = button/slider).

### F4 — The restricted HEART: rich inline paragraph + links via surgical absolute paint. HERE THE CENTRAL CONTRADICTION IS FACED.
- **Why here:** the first and only point where egui provably does not compose
  — mixed spans (bold+link+text) on the same wrapping line, with per-run hit-test.
  Before that egui suffices; after it there is no gain. F0-F3 are the net: if F4
  fails, the rest does not regress.
- **Mandatory pre-spike (1-2 days, KILL-GATE):** render ONE painter paragraph
  BETWEEN two egui blocks and prove the baseline/vertical-advance matching via
  `ui.allocate_space(galley.size())`. If the boundary does not match in N days →
  **freeze at F3** (already usable) and open an issue. Converts the biggest
  late-risk into early-risk.
- **Delivers:** ONLY the rich WRAP branch assembles ONE `egui::text::LayoutJob` (one
  `LayoutSection` per run) → `f.layout_job(job) → Arc<Galley>` → reads `galley.rows`;
  paints with `allocate_painter`; hit-tests links per galley line → returns
  NodeId via the F3 contract. Plain-text WRAP stays on `horizontal_wrapped`.
- **Reuses:** tokenizer/DOM/`ComputedStyle` in full; egui's measurer
  (`LayoutJob`) — does not implement `glyph_width`; `present()`/`SharedGpu` intact.
  **Abandons:** rich WRAP via `horizontal_wrapped`+label-per-child (composes
  "works-but-wrong": misaligned baseline).
- **Effort:** HIGH, **2-3 weeks** (cross-frame hit-test + baseline-matching
  live INSIDE here and egui doesn't solve them). **Gate/risk:** MEDIUM-HIGH,
  confined to one `render_*`.

### F5 (conditional/optional) — Galley cache + entities/selectors on demand.
- **Usable:** absolute paragraphs without re-layout/frame; entities and compound
  selectors when a fixture asks for them.
- **Delivers:** `Arc<Galley>` cache per NodeId in the `UiCtx`, invalidated by
  hash(text+style+width)+**DPI from the start** (forgetting DPI = blurry text
  when switching monitors). Only absolute galleys enter.
- **Reuses:** thread_local `UiCtx`; F0 hash. **Abandons:** the north-star's
  tree-Arc. **Effort:** low (~3-5 days), conditional on measurement.

**Honest total cost:** F0-F3 ≈ 6.5-9.5 weeks (a usable, demonstrable "rich text +
styled boxes + click" engine); F4 ≈ +2-3 weeks; F5 conditional.
The coexistence of two renderers exists **only inside the WRAP branch of
`frame.rs`** (not in the whole engine) and has a **verifiable kill-gate** (a test that
fails if rich-WRAP still falls into `horizontal_wrapped` after F4).

---

## 5) Verifiable kill-gates

Mechanisms that fail the build/test if an invariant is violated — we don't trust
manual discipline:

- **F0:** `frame.rs` split (the `read_before_commit.sh` gate already fires at > 500
  lines — use that as the split's kill-gate).
- **F4:** a test that fails if the rich-WRAP branch still falls into `horizontal_wrapped`
  after F4 (the coexistence has to die where it should).
- **F4 pre-spike:** the baseline-matching is proven in a screenshot fixture before
  committing 2-3 weeks.
- **BINARY ceiling criterion per property** (not "inline-flow works or
  not" — the current WRAP already composes "wrong", it doesn't hit a clean wall): mixed
  baseline / wrap mid-run / justify are tested individually; each one decides whether that
  case needs the F4 painter or stays on egui.

---

## 6) Risks that still scare + mitigation

1. **String handle across frames leaks/reads-after-free under mark+sweep GC.**
   Mitigation: invariant 1 (`getText` re-read, never persisted); source of
   truth is the Rust arena. Prove in the F0 `claude-egui-gettext` fixture.
2. **Stale NodeId = read of a dead node applied to a live node.** Mitigation:
   invariant 2 (version in F0).
3. **Full u64 sentinel takes the wrong branch.** Mitigation: invariant 3 (`-1`).
4. **Chaining facade doesn't compile** (dispatch on a call return without
   annotation; `new` of an imported class bails). Mitigation: invariant 5 (top-level
   functions + flattened prelude). Validated in an F0 fixture.
5. **F4 blowing the deadline** (baseline + cross-frame hit-test, outside egui).
   Mitigation: pre-spike with kill-gate; F0-F3 are an independent product; freezing
   at F3 is a formalized product decision, not failure.
6. **CSS vocabulary leaking into Rust** (the gate doesn't catch it — it's not a class
   name). Mitigation: invariant 4 (opaque slot); review criterion.

---

## 7) The first concrete slice (≤ 1 day — validates the direction) — ✅ SUPERSEDED by F0

> **NOTE (2026-06-24):** this slice was planned as a proof-of-concept of the
> egui-free `style.rs` + `defineStyle`. In practice the **entire F0 was delivered**
> (see STATUS in F0 above), including the egui-free `style.rs` with `ComputedStyle` +
> OPAQUE `apply_slot` (PR #1732) — the string→value converter and the own types already
> exist. What is missing to "see a colored pixel" is only the `defineStyle` ABI wiring
> the `apply_slot` to the render: that is **F1** above, no longer a separate slice. The
> direction is validated (the window renders HTML, mutates the DOM live, egui-free
> style compiles and the inline `style=""` already draws color/size).

Empirically prove the 3 uncertain viability points BEFORE committing to the
roadmap (the whole strategy assumes they compile).

**Files:**
- `crates/rts-egui/src/style.rs` (new, egui-free): `pub struct ComputedStyle {
  color: Option<u32>, bg: Option<u32>, font_size: Option<f32> }` + `apply_slot(&mut
  self, slot: i64, val: i64)` (slots: `0=color`, `1=bg`, `2=font_size`).
- `crates/rts-egui/src/lib.rs`: 1 ABI member `defineStyle(tag: StrPtr, slot: I64,
  val: I64) -> Void`, mold identical to `defineBlock`, via `e.ns("egui").member(...)`.
- `crates/rts-egui/src/frame.rs` (`render_inline`): read the tag's `ComputedStyle` and
  apply `RichText::new(t).color(c).size(s)`.

**ABI (hard convention):** `StrPtr` only as arg; return `Void`/`I64`; no
string getter; future sentinel = `-1`.

**TS example (`examples/claude-egui-style.ts`):**
```ts
import egui from "rts:egui";
// slots: 0=color 1=bg 2=font_size ; cores como 0xRRGGBBAA em i64
egui.defineStyle("h1", 0, 0x0088FFFF); // h1 azul
egui.defineStyle("h1", 2, 28);          // tamanho 28
egui.defineStyle("p",  0, 0xCCCCCCFF);
// ... loop run() existente desenha; valida cor+tamanho via egui, zero painter
```

**Success criterion:** the window shows a blue `h1` size 28 and a gray `p`, via
`RichText` — proving that (a) the opaque slot works without leaking vocabulary, (b)
`defineStyle` compiles in the `defineBlock` pattern, (c) the flattened facade compiles. If
any of them bails, the facade's shape is adjusted BEFORE F1.

---

## 8) Relation to the other documents

- **[`rts-html-north-star.md`](rts-html-north-star.md)** — the old 5-tree
  `PLANO.md`, frozen as the theoretical ceiling. Does NOT dictate phases. It only
  "wakes up" if the F4 binary ceiling criterion proves egui is not enough beyond the
  rich paragraph.
- **[`README.md`](README.md)** — folder index.
- **[`arquitetura.md`](arquitetura.md)** / **[`critica-adversarial.md`](critica-adversarial.md)**
  — base study (canonical pipeline, critique that cut the scope). Still
  valid as foundation; they fed the decisions above.
