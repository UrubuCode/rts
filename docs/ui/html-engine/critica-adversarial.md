# Adversarial critique of the plan

I will be direct and harsh, as requested. The plan is technically competent and the base research is good, but it commits the classic sin of someone who has never shipped a layout engine: it underestimates the three things that actually consume the time (text, real cascade, incrementality) and packages an "advanced HTML + CSS5" scope that is fantasy. Concrete critique below.

---

## 1) SCOPE — "advanced HTML + CSS5" is fantasy; the real subset is hidden in the research, not in the plan

The architecture plan is honest at some points (P1 = simple selectors, block-only), but the *title* of the task ("advanced HTML + CSS5") and the body do not match. The css-subset research delivers a sober and correct subset; **the architecture plan does not cite that subset with the same frankness** — it describes the 5-tree plumbing as if it were already the engine. Plumbing is not the work. The work is the content of each box.

What costs 10x more than the plan suggests, in order of pain:

1. **Inline/text layout (Phase 7, pushed to the end, 10% of the progress bar)** — this is 40-60% of the real effort of an engine, and the plan treats it as the last 10%. Line breaking, per-run measurement, baseline, inline font mixing, whitespace collapsing (CSS's whitespace collapsing algorithm is an infinite source of subtle bugs), `white-space`, justification. I will return to this in section 2.
2. **"Correct" cascade** — the plan says "sort by specificity, apply". That is the easy part. The hard part is shorthand expansion (`margin: 1px 2px 3px 4px`, `border: 1px solid red`, `font: ...`), `initial`/`inherit`/`unset`, percentage values that resolve at different moments (`%` of width resolves in layout, not in the cascade), and `em`/`rem`/`%`/`auto` with distinct resolution rules. The plan resolves `Em`/`Percent` "against the parent" in Phase 3 — **wrong for width/height/margin/padding in %**, which depend on the *containing block* (Phase 4), not on the parent's computed value. This is a design bug already present in `enum Dimension { Auto, Px(f32) }`: it discards `%` too early.
3. **Fonts** — the plan assumes egui handles fonts. egui handles *one* embedded family. `font-family`, fallback, font matching, web fonts, synthetic bold vs. real weight, synthetic italic — none of this comes for free. The plan mentions `weight: u16` and `italic: bool` as if they were applicable to the galley; egui has no arbitrary-weight synthesis without you supplying the font files.

**Verdict:** the *achievable* subset is that of Phase 1/2 of the css-subset research (block + basic inline, ~12 properties, simple selectors + descendant). The plan should declare this in the title and abandon "advanced/CSS5".

---

## 2) TEXT LAYOUT — the monster is acknowledged but outsourced with dangerous optimism

The plan knows text is hard (it creates the `TextMeasurer` trait), but the way it splits work between "egui measures" and "I break lines" is naive on two concrete points:

- **The `TextMeasurer` trait is badly designed.** The signature `measure(text, font_size, weight, italic) -> (w,h)` treats a run as atomic and uniform. Real text on a line is multi-run, multi-font, multi-color (`<b>`, `<span>`, `<a>`), and line breaking has to happen **across** run boundaries, not run by run. If you break run-by-run, "stays **bold** here" breaks wrongly between the normal and the bold. The egui-as-paint research already points to the correct way out — `LayoutJob` with multiple `LayoutSection` — but then **egui is the one breaking the line (via `wrap.max_width`), not you**. The plan's trait assumes the opposite (you break, egui measures granularly via `glyph_width`). You cannot have both: either you delegate breaking an entire line (multi-run) to egui's `layout_job`, or you reimplement multi-run shaping yourself. The plan talks about both paths without choosing, and the "I break with `glyph_width`" path **does not compose** with mixed inline spans.
- **Harsh recommendation:** delegate **as much as possible** to the galley. Build one `LayoutJob` per inline-context *block* (not per run), set `wrap.max_width` = content box width, and read back `galley.rows` to discover where egui broke and to position. You lose fine control (hyphenation, `text-indent` on specific lines), but you get bidi, kerning, shaping and multi-run breaking **for free and correct**. Reimplementing that in pure Rust is a months-long project by itself. The plan keeps the door open to "do it yourself with `glyph_width`/`row_height`" — **close that door**, it is a time trap.
- **What the plan does not even mention:** bidi (RTL/Arabic/Hebrew text), grapheme clusters (emoji, combining marks), whitespace collapsing, `word-break`/`overflow-wrap`. If the goal is only Latin LTR, **say so explicitly** and cut bidi from scope. Today the plan pretends `String` + `glyph_width` covers text, and it does not.

---

## 3) egui-as-paint — hit-testing is UNDERsized (it is the real Achilles' heel)

The egui-as-paint research is excellent and correctly covers the coordinate/scroll/repaint traps (`allocate_painter`, `show_viewport`, content→screen translation, recreating the galley on DPI change). **But the architecture plan barely talks about hit-testing**, and that is where immediate mode bites you:

- `allocate_painter(size, Sense::hover())` gives you **one** `Response` for the entire surface. To know *which box* was clicked (link, button, which `<a href>`), you have to do hit-testing **yourself**: keep a list of clickable rectangles + node-id, and on the next frame test `response.interact_pointer_pos()` against them. The plan mentions this in passing ("registers its Rect... matched by node-id") but does not size the work: you are rebuilding the DOM's event system (not capture/bubble, but at least "what is the click target", z-order resolving overlap, hover state for `:hover`, `pointer` cursor over links).
- **1-frame latency** already exists in the current code (`button_results`/`button_cursor`) and the plan consciously inherits it. OK for a button. **Not OK for `:hover`** — hover state delayed by 1 frame flickers. And `:hover` is in the implicit "CSS" scope. Either you cut `:hover` (recommended for the MVP) or accept that it requires reactive re-layout in the same frame, which the recomputed-from-scratch 5-tree pipeline does not support cheaply.
- **Repaint:** immediate mode repaints everything every frame. The plan covers viewport culling (good), but **does not cover tree rebuild**. Does `egui.html(string)` re-parse today? If so, you rebuild DOM→Style→Layout every frame the string changes — and since RTS is immediate-mode, TS probably calls `egui.html(...)` every frame. That is a full reflow per frame. For a small static page, fine. For anything with real text, measuring the galley of everything every frame will dominate the time. **The plan has no layout-cache-between-frames strategy**, and the ephemeral tree model (`StyledNode<'a>` with borrows from the parent tree) makes caching *harder*, not easier — lifetimes tied to the frame.

---

## 4) INCREMENTALITY — "build 5 layers before seeing a pixel" (the plan's gravest defect)

This is the most serious critique. **The first pixel rendered by the new engine only appears at step 6 of 7** (90% of the progress bar). Steps 1-5 (DOM, CSS, Style, Layout, Display list) produce **Rust structs nobody sees**. You will write ~60% of the code against unit tests before anything appears on screen. This is exactly the anti-pattern CLAUDE.md itself condemns ("deliver value in every phase").

Worse: step 4 (layout) **depends** on the `TextMeasurer`, which only truly exists at step 6. The plan "solves" this with a mocked measurer — meaning you validate block layout against *fake* text widths, and when the real measurer comes in, all the inline layout changes and you re-debug. The mock gives you a false sense of progress.

**Reorder to see a pixel early:**
1. Start with the **thinnest possible vertical path**: trivial parse (`<p>texto</p>` + `<h1>`), no CSS, no cascade, block-only, and **paint immediately** via `Painter` with galley. That is minimal DOM + minimal block layout + paint, wired end-to-end, in the first week. A real pixel on screen.
2. *Then* thicken each layer (CSS, cascade, inline, scroll). Each increment renders something new and visible.

The current plan's risk is the classic "5 trees ready, 0 pixels, and when everything is wired nothing aligns". You have no visual feedback to catch coordinate/baseline/box-model errors until the end — and those errors *only* show up visually.

---

## 5) MIGRATION — coexistence is OK on paper, but the real friction point is re-parsing and event state

This part the plan got right in its decision (the flat queue survives as "simple mode", HTML is a new and separate path, no HTML→`WidgetCmd` conversion). The calculator and the current widgets **do not break** because the new path is parallel. Good.

The real risks the plan undersizes:
- **`egui.html(str)` changes the body but the event model is incompatible.** Today buttons match by **positional index** (`button_cursor`). HTML mode wants to match by **node-id**. But the node-id is only stable if the DOM is stable between frames — and if TS re-calls `egui.html(stringDiferente)` every frame, the node-ids dance. The plan asserts "node-id is more stable than index" without showing where the id comes from: if it is generated by parse order, it is **exactly as fragile as the index**. A stable id requires explicit `id=`/`key=` in the HTML or a reconciliation scheme — which the plan does not have.
- **Two buffers in `UiCtx` (`FrameContent::Simple | Html`)** — and if the user mixes `egui.label()` with `egui.html()` in the same frame? The plan assumes exclusivity ("`endFrame` picks the walker by the content present"). Mixing is a real case (HTML + a native slider below) and the plan does not say how to compose the two walkers in the same window in the correct order.

---

## 6) CSS5/modern — explicit cut (the research already lists it, the plan does not own it)

The architecture plan **does not declare what is out**. The css-subset research declares it, and well. This list has to be in the plan, not buried in the research. **Cut explicitly and irrevocably for the MVP:**

- **Flexbox** — each formatting context is "a mini-project" (the research itself says so). Flex is iterative resolution of `grow/shrink/basis`. Out of the MVP. (And it is what people will want most — be honest that it is not there.)
- **Grid** — complete fantasy for this scope. Resolving `fr`/`minmax`/auto-placement tracks is a subsystem bigger than the entire rest of the engine. Cut and never promise.
- **`position: absolute/fixed/sticky`, `float`, real `z-index` (stacking contexts)** — out. The plan says "z-order = display list order", which is true *until* you have `z-index`/`position`, then it breaks.
- **Container queries, `:has()`, `@scope`, cascade layers `@layer`, nesting** — fantasy. The research already flags `:has()` and container queries as genuinely expensive (invalidation / circular layout↔style dependency). Never promise.
- **`transform`/`transition`/`animation`/`filter`/`clip-path`** — out. Animation requires a temporal loop + invalidation that the ephemeral pipeline does not support.
- **`var()`/custom properties** — out of the MVP (resolution step in the cascade with fallback).

Keep only: block + inline normal flow, `display: block|inline|none`, box model, ~12 paint/box properties, simple + descendant selectors, specificity + inheritance. **That is already 3-6 months of honest work.** "CSS5" disappears.

---

## THE 5 MOST SERIOUS REAL RISKS

1. **Text/inline layout undersized and badly architected.** It is 40-60% of the effort, sits as "the last 10%", and the `TextMeasurer` trait does not compose with mixed inline spans. Without rewriting the measurement boundary around egui's `LayoutJob`/`galley.rows`, this stalls the project. **Biggest single risk.**

2. **Zero pixels until 90% of the plan.** Building 5 layers with a mocked measurer before seeing the screen is a recipe for "everything ready, nothing aligns". The visual feedback that catches baseline/coordinate/box-model errors only arrives at the end.

3. **Full reflow per frame + no layout cache.** Immediate mode + ephemeral `StyledNode<'a>` tied to the frame = re-parse + re-style + re-measure of everything every frame. Real text dominates the time. There is no cache plan, and the chosen lifetimes *hinder* caching.

4. **Hit-testing and event identity.** "Which box was clicked" and node-id stable between frames are not resolved. A node-id generated by parse order is as fragile as the index. `:hover` with 1-frame latency flickers. You are rebuilding half of the DOM's event system without saying so.

5. **Unit resolution at the wrong moment.** `enum Dimension { Auto, Px(f32) }` discards `%` in Phase 3, but width/margin `%` resolves against the *containing block* in Phase 4. Design bug already in the struct. Computed values have distinct resolution moments (`em` early, `%` late) — the plan collapses the two.

## WHAT TO CUT (mercilessly)
Flex, grid, position/float/z-index, container queries, `:has()`, `@layer`, nesting, var(), transform/transition/animation, bidi/RTL, web fonts, font fallback. Everything "CSS5". Reduce selectors to simple+descendant and properties to ~12.

## WHERE THE PLAN NEEDS TO BE MORE HUMBLE
- Replace the "advanced HTML + CSS5" title with "static block+inline HTML/CSS subset, LTR, single font".
- Admit that **egui does the text** (via `LayoutJob`/`galley`), not that you do it with `glyph_width`. Close the "I'll do it myself" door.
- Invert the order: **pixel in the first week** (thin vertical path), layers thickened afterwards — not 5 trees before the first pixel.
- Resolve `%` in layout, not in the cascade. Fix `Dimension` to carry `Percent`.
- Have an answer for **re-parse/cache between frames** and for **stable node identity** (explicit id/key), before writing Phase 4.

The 5-tree skeleton is right (it is the canonical pipeline). The plan's error is not the architecture — it is confusing having the plumbing ready with having an engine, underestimating text, and not delivering a pixel until the end.
