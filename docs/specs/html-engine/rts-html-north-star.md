# RTS HTML+CSS render engine — TARGET ARCHITECTURE (5 trees; reactivated 2026-06-27)

> ## ✅ REACTIVATED AS THE OFFICIAL DIRECTION (2026-06-27)
> This document **is once again the engine's target architecture**. On 2026-06-27 the
> developer (Marcos) decided to *"process everything in the DOM and egui only reads and displays"*,
> **reverting decision #2 of the roadmap** (which put layout in egui). The 5
> trees described here (DOM→Style→**Layout**→DisplayList→Paint, with egui only
> as the paint+measurement backend) are now the pipeline to build. Reason: layout
> in egui makes it **impossible to swap the UI** and leaves the headless DOM without POSITION.
> Work follows the phases P0–P7 below. See the memory `project_layout_moves_to_dom`
> and the reversal note at the top of `rts-html-roadmap.md`.
>
> 🧭 **ORIGINAL ROLE (decision of 2026-06-23, now SUPERSEDED):** ~~this is the
> north-star — a frozen conceptual reference, not an execution plan, nobody
> picks work from here~~. That held from 2026-06-23 to 2026-06-27, while the
> strategy was the light engine with egui-layout. **It no longer holds** — see above.
>
> **Crate:** the document was written when the target was a new `rts-html` crate.
> Today the DOM/style already live in the **`rts-dom`** crate (extracted from rts-egui) — so
> the Layout/DisplayList phases are ADDITIVE to the existing `rts-dom` (which already has
> DOM+Style+cascade+`<style>` tag), not a crate from scratch. Trust the real `rts-dom`.

> Historical status (when it was the `PLANO.md`): actionable PROPOSAL (v1).
> Code language: Rust (identifiers in English). Communication: Portuguese.
> This document incorporates the 5-tree architecture AND the corrections demanded
> by the skeptical critique. Where the critique pointed, the plan regressed to something
> humble, incremental, and honest — that is deliberate, not laziness.

---

## ⚠️ IMPLEMENTATION STATUS (2026-06-23) — conscious divergence from the plan

> Mandatory annotation (RULE #0: never let the spec lie). A retained HTML render
> engine was implemented on main **through a DIFFERENT path than the one described
> below**. It is not "P0..P7 partially done" — it is an **alternative, lighter
> architecture** that covers part of the plan's goals and diverges on central points.
> Origin branch: `feat/egui-dom-tree` (merged to main). Crate: `rts-egui` (NOT
> the `rts-html` the plan asked for).

### What was done (in `rts-egui`, not in `rts-html`)

- **Retained tree DOM** (`rts-egui/src/dom.rs`): arena `Vec<Node>` + stable
  `NodeId`, parent/children, **attributes preserved** (`class`/`id`/`href`…),
  devtools-style `Dom::dump()`, **O(1) `id`/`class` indices** for query.
- **Render traverses the tree** (`frame.rs::render_dom`).
- **Dynamic block allocator** (`block.rs`): a `tag → layout` map defined in
  **TS** via `egui.defineBlock`/`defineInline` (display vertical/wrap/horizontal/
  grid + indent + prefix + flags). The engine does not name tags.
- **Mutation via JS** (`egui.querySelector/setText/setAttr/createElement/
  appendChild/removeNode`) — foundation for runtime DOM manipulation.
- **Inspection on the TS side**: `egui.domDump`.
- Tests: `cargo test -p rts-egui` (18). Examples: `examples/egui_html_basico.ts`,
  `egui_html_tree_complexa.ts`, `egui_dom_mutacao.ts`.

### Where it DIVERGES from the plan below (we did not do the same)

| Axis | PLANO.md (below) | Implemented (main) |
|---|---|---|
| **Crate** | NEW crate `rts-html`, pure Rust, zero egui dep | inside `rts-egui` |
| **Pipeline** | 5 trees (DOM→Style→Layout→DisplayList→Paint) | 2 stages (DOM → direct render) |
| **Paint** | `allocate_painter` + ABSOLUTE Painter + own box model | `ui.label`/`horizontal_wrapped`/`Grid` — **egui DOES the layout** (the plan §3 explicitly forbids this) |
| **CSS** | real cascade: specificity + inheritance + `%`/`em` resolved in distinct phases | tag→layout map in TS (`defineBlock`); **no** cascade/inheritance/box model/`%` |
| **Events** | hit-testing by `node_id`, click on `<a>`/`<button>` | **there is no** click/hit-testing yet; there is programmatic mutation via JS |
| **Inline text** | multi-run `LayoutJob` measured+wrapped by egui (P4, the "heart") | `RichText` per fragment, egui positions |

### Honest assessment

The current implementation is a **LIGHT data-driven retained renderer** (the
philosophy the user asked for: "everything derives from inline/block", "blocks
defined in TS", "DOM optimized beyond the standards"). It is **NOT** the canonical
browser engine of this plan. From the plan, it covers: the tree topology (P0),
attributes (part of P5), and brings forward DOM manipulation (not planned here).
It does **NOT** cover: box model, CSS cascade, absolute paint, multi-run
inline-flow, scroll, hit-testing/events — items P1(absolute paint)→P7.

**Open decision for the devs:** either (a) this light path replaces the 5-tree
plan as the official direction (and this PLANO.md is rewritten/retired), or
(b) the 5-tree plan remains the long-term target and the light path is an
intermediate/coexisting stage. Until decided, BOTH documents
hold and this note prevents someone from implementing P1..P7 thinking they start from scratch.

---

---

## 0) HONEST summary and scope

### 0.1) What this IS

An **in-house render engine** for a **static** subset of HTML+CSS,
embedded in `rts-egui`, that reproduces the canonical browser pipeline (DOM → Style
→ Layout → Display list → Paint) in pure Rust, using egui **only as
a paint backend, text measurement, and scroll** — never as a layout engine.

The realistic target, stated without euphemism:

- **HTML**: structural subset — nested block and inline tags
  (`div`, `p`, `h1..h6`, `span`, `b`/`strong`, `i`/`em`, `a`, `ul`/`li`, `br`,
  `img`), attributes (`id`, `class`, `style`, `href`, `src`), text with basic
  entities. Unknown tag becomes a generic `Element` (not discarded).
- **CSS**: the subset of the css-subset research Phase 1+2 — simple
  selectors + descendant, ~12 text/box properties, real cascade
  (specificity + inheritance + order), normal-flow block+inline box model.
- **Layout**: **block + inline in normal flow, LTR, single font**. Full box
  model (content/padding/border/margin). Line breaking and text shaping
  **delegated to egui** (`LayoutJob`/`Galley`).
- **Paint**: flat display list → `egui::Painter` in absolute coordinates,
  with `ScrollArea::show_viewport` (viewport culling) and own hit-testing
  for clicks on `<a>`/`<button>`.

"Advanced" here means, and **only** means: **real CSS cascade with inheritance and
specificity + correct block/inline box model + scroll + clickable links**.
That is already 3–6 months of honest work. It is an attainable and useful target — a
"rich text with boxes" renderer, not a browser.

### 0.2) What this is NOT — explicit cuts, no comeback in the MVP

The critique is right: "advanced HTML + CSS5" is fantasy. Cut from the start,
by name, so nobody promises what they can't deliver:

| Cut | Why (1 line) |
|---|---|
| **Flexbox** | each formatting context is a mini-project; iterative grow/shrink/basis resolution |
| **Grid** | `fr`/`minmax`/auto-placement track resolution is bigger than the whole rest of the engine |
| **`position: absolute/fixed/sticky`, `float`** | require a containing block outside the flow + removal from normal flow |
| **Real `z-index` / stacking contexts** | z-order = display list order **only works without `position`/`z-index`** |
| **Animations / `transition` / `transform` / `filter` / `clip-path`** | require a temporal loop + invalidation the ephemeral pipeline does not support |
| **`:has()`, container queries, `@scope`, `@layer`, nesting** | expensive invalidation / circular layout↔style dependency |
| **`var()` / custom properties** | extra resolution pass in the cascade with fallback |
| **Reactive `:hover`/`:focus`** | requires re-layout in the same frame; 1-frame latency flickers (see §6 risk 4) |
| **Sibling `+`/`~`, `:nth-child`** | traverse siblings; re-evaluation on child-list change |
| **bidi / RTL / Arabic / Hebrew** | bidirectional shaping; we explicitly assume **Latin LTR** |
| **complex grapheme clusters / combining marks** | delegated to egui to the extent egui already handles them; no in-house treatment |
| **`font-family` / fallback / web fonts / arbitrary weight synthesis** | egui resolves **one** embedded family; see also §6 risk |
| **`box-sizing: border-box`** (in the MVP) | default `content-box` only; `border-box` enters a late phase if time remains |

These cuts are **permanent for the MVP** and never "promised for later" without
re-justification. Flex and grid especially: let's be honest that we **don't have them** —
it is what people will want most, and saying "coming in v2" would be a roadmap
lie.

### 0.3) Project doctrine (PRIMORDIAL-vs-Registry)

HTML/CSS **have no native syntax** in JS/TS → `rts-codegen-new` **NEVER**
names them. The engine lives in Rust as a **render primitive** exposed by a
`NamespaceMember` in `abi::SPECS` (`egui.html`, same as `io.print`). The
high-level logic (building the string, reacting to events) is TS. The codegen
engine only resolves a generic namespace call — the same boundary as `io`/`fs`/`ui`.
None of this crosses the codegen.

---

## 1) Architecture — the phases and the main Rust structs

Canonical pipeline (web.dev / Servo / robinson), **five chained trees that
only flatten at the end**:

```
HTML bytes → DOM tree → (+ Stylesheet) → Style tree → Layout tree → Display list → Paint
   parser      árvore       cascade+        computed       box model    lista plana    egui Painter
              de nós       herança         (sem %)         (resolve %)   (absoluto)     + medição
```

The tree persists from Phase 1 to Phase 4 because ancestry, inheritance, and
constraint-solving require the parental topology. It only flattens into the display list,
**after** inheritance/cascade/constraint have consumed the topology.

> **Correction from the critique embedded right here (risk 5):** `%` and `auto` of
> width/margin/padding resolve against the **containing block in Phase 4**, not
> against the parent's computed in Phase 3. `em`/`rem` resolve early (Phase 3, against the
> parent's `font-size`); `%`/`auto` resolve late (Phase 4). The two resolution
> moments are distinct and the struct reflects this (see `Dimension` below).

### Phase 1 — HTML → DOM tree (`rts-html/src/dom/`)

```rust
// dom/node.rs
pub struct DomNode {
    pub id: NodeId,                    // estável-por-parse (ver §3 e §6 risco 4)
    pub node_type: NodeType,
    pub children: Vec<DomNode>,
}
pub enum NodeType { Element(ElementData), Text(String) }
pub struct ElementData {
    pub tag_name: String,
    pub id_attr: Option<String>,       // de attributes["id"], cache p/ match O(1)
    pub classes: Vec<String>,          // split de attributes["class"]
    pub key: Option<String>,           // de attributes["key"] — identidade estável (§6 risco 4)
    pub attributes: HashMap<String, String>,
}
pub type NodeId = u32;
```

Recursive-descent parser over the char-by-char `Parser { pos, input }` already
existing in `html.rs::tokenize`. `<p><b>x</b></p>` produces
`Element(p) → [Element(b) → [Text("x")]]`. Unknown tag → generic `Element`.
Entities decoded in the `Text` node.

### Phase 2 — CSS → Stylesheet (`rts-html/src/css/`)

```rust
// css/stylesheet.rs
pub struct Stylesheet { pub rules: Vec<Rule> }
pub struct Rule {
    pub selectors: Vec<Selector>,      // ordenados por especificidade desc no parse
    pub declarations: Vec<Declaration>,
}
pub enum Selector {                    // P1: Simple; P5: Compound com combinador descendente
    Simple(SimpleSelector),
    Descendant(Vec<SimpleSelector>),   // direita-p/-esquerda; só na fase tardia
}
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class: Vec<String>,
}
pub struct Declaration { pub name: String, pub value: Value }
pub enum Value {
    Keyword(String),                   // "block", "bold", "left"
    Length(f32, Unit),                 // PRESERVA a unidade — NÃO resolve aqui
    Color(Color),
}
pub enum Unit { Px, Em, Rem, Percent } // Percent SOBREVIVE até o layout
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

`Selector::specificity() -> (u32, u32, u32)` (IDs, classes, types), ordered at
parse time. CSS robustness: on error, discard only the unrecognizable part and continue.

### Phase 3 — Style tree (cascade + inheritance, WITHOUT resolving `%`/`auto`)

```rust
// style/styled_node.rs
pub struct StyledNode<'a> {
    pub node: &'a DomNode,
    pub specified: PropertyMap,        // saída da cascata p/ este nó
    pub computed: ComputedStyle,       // herança aplicada; em/rem resolvidos; % NÃO
    pub children: Vec<StyledNode<'a>>,
}
pub type PropertyMap = HashMap<String, Value>;

pub struct ComputedStyle {
    pub display: Display,              // Block | Inline | None
    // herdados (texto):
    pub color: Color,
    pub font_size_px: f32,            // em/rem JÁ resolvidos contra o pai (cedo)
    pub font_weight: u16,
    pub italic: bool,
    pub text_align: TextAlign,
    pub line_height: LineHeight,
    // não-herdados (box) — % PRESERVADO p/ resolver na Fase 4:
    pub width: Dimension,
    pub height: Dimension,
    pub margin: Edges,
    pub padding: Edges,
    pub border: Edges,
    pub border_color: Color,
    pub background: Option<Color>,
}
pub enum Display { Block, Inline, None }

// CORREÇÃO DA CRÍTICA (risco 5): Dimension carrega Percent até o layout.
pub enum Dimension { Auto, Px(f32), Percent(f32) }
pub struct Edges { pub top: Dimension, pub right: Dimension, pub bottom: Dimension, pub left: Dimension }
```

Algorithm (`style/cascade.rs`): `matching_rules` collects
`(specificity, &Declaration)` from the rules that match (O(1) AND of tag/id/classes
on the node itself); sorts by ascending specificity; applies in order (tie →
source order). `resolve_inherited` copies the missing inherited properties from
`parent.computed` and resolves `em`/`rem` against the parent's `font_size_px`.
**`%` and `auto` are NOT touched here.** `display:none` marks the node for
exclusion from Phase 4.

### Phase 4 — Layout tree (box model; resolves `%`/`auto`; measures via egui)

```rust
// layout/box.rs
pub struct LayoutBox<'a> {
    pub node_id: NodeId,               // p/ hit-testing por id (§3)
    pub dimensions: Dimensions,
    pub box_type: BoxType<'a>,
    pub children: Vec<LayoutBox<'a>>,
}
pub enum BoxType<'a> {
    Block(&'a StyledNode<'a>),
    Inline(&'a StyledNode<'a>),
    Anonymous,                         // embrulha inlines mistos sob um pai block
}
pub struct Dimensions {
    pub content: Rect,                 // x,y,w,h FINAIS (% e auto já resolvidos)
    pub padding: EdgeSizes,            // f32 absolutos
    pub border:  EdgeSizes,
    pub margin:  EdgeSizes,
}
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
pub struct EdgeSizes { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32 }
```

Block algorithm (robinson Part 6), **a single traversal**:
1. **Widths (top-down)**: here `%` resolves against the containing block's
   width (= the parent's content width). `auto` expands/absorbs underflow.
2. **Position (x,y)**: box below the previous ones in the container.
3. **Heights (bottom-up)**: parent height = sum of children's heights, unless
   `height` is explicit.

**Inline** calls egui via `TextMeasurer` (§2/§3): builds a `LayoutJob` per
**inline context block** (not per run), asks egui to measure+wrap with
`wrap.max_width = content box width`, and reads `galley.rows` to position the
lines. The `Galley` is kept for paint (no re-shaping).

### Phase 5 — Display list (`rts-html/src/paint/display.rs`)

```rust
pub enum DisplayItem {
    SolidRect { rect: Rect, color: Color },
    Border    { rect: Rect, edges: EdgeSizes, color: Color },
    Text      { x: f32, y: f32, galley: GalleyHandle, color: Color },
    Image     { rect: Rect, src: String },
}
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
    pub hit_rects: Vec<HitRect>,       // p/ hit-testing (§3)
}
pub struct HitRect { pub rect: Rect, pub node_id: NodeId, pub kind: HitKind }
pub enum HitKind { Link(String), Button(NodeId), None }
```

`GalleyHandle` is an opaque index to an `Arc<Galley>` kept on the
`rts-egui` side (the `rts-html` struct does not know the egui type — see §2 trait). The
order of the `Vec` **is** the z-order (Painter draws back-to-front; without `z-index`/
`position` in the MVP this is sufficient and correct).

### Phase 6 — Paint (in `rts-egui`, NOT in `rts-html`)

A walker translates each `DisplayItem` into an `egui::Painter` call, adding the
origin (`response.rect.min`) and the scroll offset:
- `SolidRect` → `painter.rect_filled(rect, CornerRadius::ZERO, color)`
- `Border` → `painter.rect_stroke(rect, ZERO, stroke, StrokeKind::Inside)`
- `Text` → `painter.galley(pos, galley, color)`
- `Image` → `painter.image(tex, rect, uv, tint)`
- `overflow` → `painter.with_clip_rect(rect)`

---

## 2) Where each layer lives

| Layer | Crate | Language | Responsibility | Depends on egui? |
|---|---|---|---|---|
| High-level API `html(str)`, stylesheet, handlers | `rts-shared/src/stdlib/html.ts` | TS | ergonomic surface over `egui.html` | no |
| **DOM + CSS + Style + Layout + Display list** | **`rts-html` (NEW)** | pure Rust | Phases 1–5; tree; constraint solving | **NO** (zero egui/winit/wgpu dep) |
| Window backend + paint + measurement + scroll + hit-test | `rts-egui` | Rust | Phase 6: walker; `TextMeasurer` impl; `ScrollArea`; event loop; wgpu | yes |
| ABI primitive `egui.html(ptr,len)` | `rts-egui` extern "C" | Rust | entry point: string → `rts-html` → display list in the `UiCtx` | yes |

**Dependency inversion** (Phase 4 needs to measure text, but `rts-html` cannot
depend on egui). The trait lives in `rts-html`, the impl in `rts-egui`:

```rust
// em rts-html — abstração, sem nada de egui
pub trait TextMeasurer {
    /// Mede e quebra um BLOCO inline multi-run em max_width; devolve as linhas
    /// já posicionadas + um handle de galley opaco p/ o paint.
    fn layout_inline(&self, runs: &[InlineRun], max_width: f32) -> InlineLayout;
}
pub struct InlineRun {
    pub text: String,
    pub font_size_px: f32, pub weight: u16, pub italic: bool, pub color: Color,
}
pub struct InlineLayout {
    pub rows: Vec<InlineRow>,          // de galley.rows
    pub galley: GalleyHandle,          // opaco; rts-egui resolve p/ Arc<Galley>
    pub size: (f32, f32),
}
```

`rts-html` is testable in isolation with a mock `TextMeasurer` (synthetic
widths) **only for unit-testing block geometry** — never as a proxy for
"feature progress" (see §5/§6 risk 2: the real pixel uses the real measurer
early). `rts-egui` implements `layout_inline` by building a `LayoutJob` with
one `LayoutSection`+`TextFormat` per run and calling `fonts(|f| f.layout_job(job))`.

Each phase is a folder (`dom/`, `css/`, `style/`, `layout/`, `paint/`) with
`mod.rs` + submodules, respecting the project's 500 lines/file ceiling.

---

## 3) The egui turning point — from automatic layout to absolute paint + hit-testing

Today `frame.rs::drenar` does **egui-as-layout**: `ui.label()`/`ui.button()` +
`ui.horizontal`/`ui.horizontal_wrapped`, and egui decides the positions. That is
correct for the **simple mode** and remains untouched.

In **HTML mode**, we stop stacking the instant an in-house box model exists:
the engine computes `x,y,w,h` (Phase 4) and `rts-egui` switches to **painting in
absolute coordinates**.

The concrete transition:

- **Surface**: `let (response, painter) = ui.allocate_painter(size, Sense::click())`.
  `response.rect.min` is the `(0,0)` origin of the box model.
- **DO NOT use** `ui.horizontal/vertical/Grid/Frame`/`RichText` in the content — they
  position on our behalf and fight the box model. Only `allocate_painter` + `Painter`
  + `ScrollArea::show_viewport`.
- **egui becomes four services** (the ones we don't want to rewrite):
  1. **Text measurement + line-breaking** via `LayoutJob`/`layout_job` — **egui
     breaks the line** (see §6 risk 1). We read `galley.rows`.
  2. **Font atlas + rasterization** — managed by epaint; `Galley` consumed
     directly in `painter.galley`.
  3. **Absolute Painter** — `rect_filled`/`rect_stroke`/`galley`/`image` in
     screen coordinates.
  4. **ScrollArea with virtual viewport** — `show_viewport(ui, |ui, vp: Rect| ...)`
     gives **culling for free** (essential in immediate mode, which re-paints everything every
     frame). content→screen translation: `screen = content + (ui.min_rect().min - vp.min)`.

### Link-click HIT-TESTING (sized, not "in passing")

The critique is right: `allocate_painter` gives **one** `Response` for the whole
surface; knowing **which box** was clicked is our work. The protocol:

1. During the display list walk, every clickable item (`<a href>`, `<button>`)
   registers a `HitRect { rect, node_id, kind }` in `DisplayList.hit_rects` (in
   content coordinates).
2. In the frame, after painting, we get the pointer position relative to the
   content: `pointer_content = response.interact_pointer_pos()? - origin + scroll_off`.
3. **Hit-testing in reverse display-list order** (top-most first, since
   z-order = paint order): the first `HitRect` containing the point is the
   target. Resolves overlap correctly without `z-index`.
4. If `response.clicked()` and there is a `Link(href)`/`Button(id)` target, we register the
   event in the `UiCtx` for TS to query on the next frame (same 1-frame-latency
   pattern already used by `button_results`/`button_cursor`).
5. **`pointer` cursor** over a link: if there is a `Link` target under the pointer,
   `ctx.set_cursor_icon(CursorIcon::PointingHand)`.

**Target identity** (see §6 risk 4): matching is by **`node_id`**, not by
positional index. The `node_id` is stable-per-parse; for stability **across
frames** when the string changes, the HTML can declare an explicit `key="..."`/`id="..."`,
and the `UiCtx` maps events by that key. Without `key`/`id`, matching
holds only within the frame (click→action on the same content), which is the common
case. We do not promise differential DOM reconciliation in the MVP.

**`:hover` stays out of the MVP** (cut §0.2): hover with 1-frame latency flickers,
and reactive re-layout in the same frame is not cheap in the ephemeral pipeline. When it
enters, it will be via re-layout on the next frame with the pointer known — a late
phase, optional.

Golden rule: **egui never sees the tree.** It only receives `Rect`/`Pos2`/`Galley`
already computed and measures text when asked.

---

## 4) CSS subset per phase

| Area | Phase 1 (text / paint, O(1) match) | Phase 2 (box model + normal flow) | Phase 3 (*attainable* advanced) | NEVER (MVP) |
|---|---|---|---|---|
| **Selectors** | tag, class, id, `*`, compound (`div.a#b`), `,` list | (same) + inheritance applied | **descendant** ` ` (right→left), `[attr]`/`[attr=val]` | sibling `+`/`~`, `:nth-child`, `:hover`/`:focus`, `:has()` |
| **Text properties** | `color`, `font-size`, `font-weight`, `font-style`, `text-align`, `line-height`, `visibility` | (inherited via the cascade) | — | `font-family`/fallback/web fonts, `letter-spacing`, `text-shadow` |
| **Box properties** | — | `display: block/inline/none`, `width`, `height`, `margin`, `padding`, `border`(-width/-style/-color), `background`/`background-color` | `position: relative` (simple offset, without leaving the flow), `overflow: hidden/scroll` (clip) | `position: absolute/fixed/sticky`, `float`, `z-index`, `box-sizing: border-box` (maybe late) |
| **Units** | `px`, keyword, `#rgb`/`#rrggbb` | `%` (resolves at layout), `em`/`rem` (resolves at style) | `auto` (margin/width) | `vw`/`vh`/`ch`/`vmin`, `calc()`, `var()` |
| **Layout** | none (text paint only) | single-pass normal-flow block+inline | `position: relative`, clipping | flex, grid, multi-pass, 2D constraint solving |
| **Cascade** | O(1) match, specificity (ID,CLASS,TYPE), sort by specificity→order | + inheritance, computed values | + `!important`, UA/user/author origins | reactive invalidation, custom props, `@layer`, container queries |
| **At-rules** | — | `@media`/`@supports` (boolean gate over viewport) | — | `@container`, `@scope`, `@font-face`, nesting |

**What NEVER enters** (recapping §0.2, now glued to the table): flex, grid,
`position: absolute/fixed/sticky`, `float`, real `z-index`, animations/
transition/transform/filter/clip-path, `:has()`, container queries, `@layer`,
nesting, `var()`, bidi/RTL, web fonts, font fallback, arbitrary weight synthesis.

---

## 5) Implementation phases P0 → P7

The principle governing the order, dictated by the critique (risk 2): **pixel in the
first week**. We do not build 5 trees before seeing the screen. The thinnest
vertical path (trivial parse → minimal block layout → real paint with galley)
connects end-to-end **before** thickening any layer. Each P delivers something
**visible** and coexists with the current widget/calculator mode.

> **Coexistence (all phases):** the simple mode (`egui.label/button/slider`,
> `frame.rs::drenar`, index-based pairing) is NOT touched. The calculator and
> current widgets keep working because HTML mode is a **parallel and new**
> path: `egui.html` does NOT emit `WidgetCmd`, it emits `DisplayList`.
> The `UiCtx` gains `FrameContent { Simple(Vec<WidgetCmd>) | Html(DisplayList) }`;
> `endFrame` picks the walker by the content present. (Mixing simple+HTML in the
> same frame: see §6 risk / §7 — out of the MVP, composition defined later.)

---

### P0 — DOM tree from the current parser, WITHOUT changing the render

**Goal:** create the `rts-html` crate, migrate `html.rs::tokenize` to a
recursive-descent parser producing `DomNode`. The current render (`Vec<WidgetCmd>`)
stays exactly as today — `egui.html` still uses the old path.

**Visible early:** nothing on screen changes (on purpose). But there is an `rts html-dump`
(or unit test) that prints the DOM tree of a string — the first tangible
evidence that the parental topology exists.

**Risk gate:** the recursive parser may regress what `tokenize` already did
(entities, unknown tags). Mitigate: parity tests against the current output of
`parse_html_to_cmds` for a corpus of real strings.

**Validates:** `cargo test -p rts-html` (isolated DOM, no egui); `rts-egui`
compiles and runs the same as before (zero regression in widget mode and in the old
`egui.html`).

`[▰▱▱▱▱▱▱▱▱▱] 10%`

---

### P1 — THIN vertical path: trivial parse + minimal block layout + real PAINT

**This is the "pixel in the first week".** It is NOT "CSS parser first". It is wiring
end-to-end the absolute minimum that puts a new pixel on screen through the new engine.

**Goal:** for `<h1>`/`<p>texto</p>` (no CSS, no cascade, block-only),
build DOM → a trivial block `LayoutBox` (stacks vertically, width =
viewport) → `DisplayItem::Text` with a **REAL galley measured by egui** → paint
via `allocate_painter` + `painter.galley`. Hardcoded font/color defaults.

**Visible early:** the new engine draws "Olá" on screen, at an absolute coordinate
**we** computed, with genuinely measured text. First pixel of the engine. A
flag (`egui.htmlEngine("v2")` or a sentinel string) picks the new path;
the old `egui.html` remains the default until the new engine covers what it covered.

**Risk gate:** coordinate/baseline errors only show up visually — that is
why we paint now, not at the end. The real `TextMeasurer` enters **right here**
(not mocked), so layout is not validated against fake widths.

**Validates:** visual comparison (screenshot) of "Olá" aligned; `response.rect.min`
anchoring the origin; a two-line paragraph wrapping at viewport width
via `galley.rows`.

`[▰▰▱▱▱▱▱▱▱▱] 20%`

---

### P2 — CSS parser + text-only Style tree (color / font), applied to paint

**Goal:** CSS parser (Phase 1 subset: SimpleSelector + `Declaration` +
`Value` keyword/length/color) → `Stylesheet`; O(1) cascade + specificity +
text **inheritance** → `ComputedStyle` (text fields only). The P1 paint starts
reading `color`/`font-size`/`font-weight`/`font-style`/`text-align` from the
`ComputedStyle` instead of defaults.

**Visible early:** `<p style="color:red">` and a rule `h1 { color: blue }`
change the color/size on screen. Visible inheritance: `<div style="color:green"><p>`
inherits green. First proof that the **tree** delivers what the flat queue could not
(inheritance).

**Risk gate:** `em`/`rem` resolved against the parent in the style tree; do **not**
touch `%` (no box model yet). Cascade ties break by source order.

**Validates:** cascade tests (more-specific rule wins; inheritance copies from the
parent; `em` resolves against the parent's `font-size`); screenshots of color/size/weight.

`[▰▰▰▱▱▱▱▱▱▱] 35%`

---

### P3 — Block box model: margin / padding / border / background / width(%)

**Goal:** full block Phase 4. `ComputedStyle` gains the box fields;
`Dimension` carries `Percent`. Layout resolves widths top-down (`%` against
the containing block, `auto` absorbs underflow), positions, heights bottom-up.
The display list gains `SolidRect` (background) + `Border`.

**Visible early:** boxes with colored backgrounds, padding pushing text
inward, margins separating blocks, borders drawn. `width: 50%` takes half.
The page starts looking like a page.

**Risk gate (risk 5):** `%`/`auto` resolved **here**, not in the cascade.
Vertical margin collapse between blocks is a source of bugs — implement the
simple version (no collapse) first and flag collapse as an increment.

**Validates:** geometry fixtures (`tests/` with a mock `TextMeasurer` for
deterministic geometry) comparing expected `content.{x,y,width,height}`; screenshots
of nested boxes with padding/margin/border.

`[▰▰▰▰▰▱▱▱▱▱] 50%`

---

### P4 — Inline flow + links + HIT-TESTING (the heart)

**This is the heart and the single biggest risk (risk 1).** Inline/text layout is
40–60% of the real effort. Here egui does the heavy lifting.

**Goal:** real inline flow. A block with mixed inline children
(`texto <b>bold</b> <a>link</a>`) becomes **one `LayoutJob` per inline context**
(one `LayoutSection`+`TextFormat` per run), measured and **wrapped by egui**
(`wrap.max_width = content width`); we read `galley.rows` to position. Links
`<a href>` register `HitRect`; click resolved by `node_id` in reverse display-list
order; `pointer` cursor over links; event exposed to TS.

**Visible early:** a paragraph with **bold** and a [blue link] in the middle, breaking
lines correctly **across** run boundaries (not run-by-run). Clicking the
link fires a TS handler. This is the proof that the measurement boundary was
drawn right (around `LayoutJob`, not `glyph_width`).

**Risk gate:** the door "I break the line with `glyph_width`" is
**CLOSED** (§6 risk 1) — breaking run-by-run breaks mixed spans. We delegate the
wrapping of each entire block to `layout_job`. Whitespace collapsing: simple
version (collapses whitespace runs) first. Stable node-id across frames: via
explicit `key`/`id` (§3); without it, intra-frame matching.

**Validates:** screenshot of correct multi-run line breaking; hit-testing test
(click inside/outside the link rect, overlap resolved by
reverse order); TS handler receives the `href`.

`[▰▰▰▰▰▰▰▱▱▱] 70%`

---

### P5 — More CSS: descendant selector, `@media`, scroll, `<ul>/<li>/<img>`

**Goal:** **descendant** selector ` ` (right→left matching walking up
ancestors), `[attr]`/`[attr=val]`, `@media`/`@supports` (gate over viewport),
`!important` + origins. `ScrollArea::show_viewport` for tall pages (viewport
culling). Lists (`<ul>/<li>` with marker) and images (`<img>` →
`DisplayItem::Image`).

**Visible early:** a real scrollable page, with a rule `nav a { color: ... }`
working (descendant), lists with bullets, an image. It starts rendering
real documents.

**Risk gate:** the descendant selector walks up ancestors — O(depth), but
still cheap; long chains are what hurts (do not implement combinators that are
not in the table). Scroll requires allocating the total `content_size` so the bar sizes itself.

**Validates:** scroll screenshots (correct culling: only items in the viewport
painted), descendant matching/not-matching, `@media` toggling rules on/off.

`[▰▰▰▰▰▰▰▰▱▱] 80%`

---

### P6 — Layout cache across frames + stable identity (`key`)

**Goal (risk 3):** immediate mode re-paints everything every frame; if TS calls
`egui.html(string)` every frame, today we re-parse+re-style+re-measure
everything. Implement a cache: hash of the HTML+CSS string → if unchanged, **reuse the
display list and galleys** from the previous frame (re-painting is cheap; re-layout
is not). Explicit `key`/`id` gives stable node identity for events and for
selective invalidation.

**Visible early:** no visual change — a change in **time**. Measure frames/s with
a real text page: before (reflow per frame) vs after (cache). The gain is
the deliverable.

**Risk gate:** the ephemeral lifetimes (`StyledNode<'a>` borrowing from the DOM)
**get in the way** of the cache (the critique pointed this out). Mitigation: the cache stores the
`DisplayList` + galleys (owned, `Arc`), not the borrowed tree; the tree is
rebuilt only when the string changes. DPI invalidation: recreate galleys
when `pixels_per_point` changes.

**Validates:** FPS benchmark with a static page (cache hit ~0 layout
work); correctness: changing the string invalidates and re-renders.

`[▰▰▰▰▰▰▰▰▰▱] 90%`

---

### P7 — Subset polish: `position: relative`, `overflow` clip, fine cuts

**Goal:** the "attainable advanced" items of table §4 Phase 3: `position:
relative` (offset that does **not** leave the flow), `overflow: hidden/scroll` (clip via
`with_clip_rect`), and whatever remains of the subset (vertical margin collapse,
`box-sizing: border-box` if there is breathing room). Switch the `egui.html` default
to the new engine when it covers everything the old one covered, and **delete** the
old `parse_html_to_cmds` path ("no legacy code" rule).

**Visible early:** `position: relative` offsets a box; `overflow: hidden`
clips content; the new engine becomes the default with no regression on what the old one did.

**Risk gate:** deleting the old path is the controlled regression — only after
proven parity. Explicitly document the regression/cutover in the PR.

**Validates:** parity fixture suite (every HTML the old one rendered,
the new one renders equally or better); `cargo test -p rts-html` + `rts.exe test`;
screenshots of `position: relative`/`overflow`.

`[▰▰▰▰▰▰▰▰▰▰] 100% ✅ — new engine is the default, old path deleted`

---

## 6) The 5 most serious risks and mitigation

### Risk 1 — Text/inline layout is 40–60% of the effort and the naive `TextMeasurer` does not compose

**The danger:** treating a run as atomic/uniform (`measure(text, size, weight,
italic) -> (w,h)`) and breaking the line **run-by-run**. Real text on a line is
multi-run/multi-font/multi-color (`<b>`, `<span>`, `<a>`), and the break happens
**across** run boundaries. Breaking run-by-run breaks "stays **bold** here"
between the normal and the bold.

**Mitigation (decision made, door closed):** egui does the text. The measurement
boundary is `layout_inline(runs, max_width)` → builds **one `LayoutJob` per
inline context block** with one `LayoutSection`+`TextFormat` per run, sets
`wrap.max_width = content width`, and reads `galley.rows` to find out where egui
broke. We get shaping, kerning, and multi-run wrapping **correct and for free**.
The alternative "I break with `glyph_width`/`row_height`" is **discarded** — it is
a months-long trap that does not compose with inline spans. We lose hyphenation and
per-line `text-indent`; acceptable (they are out of scope).

### Risk 2 — Zero pixels until the end ("5 trees ready, nothing aligns")

**The danger:** building DOM→CSS→Style→Layout→Display with a mocked measurer before
seeing the screen validates geometry against fake widths; baseline/
coordinate/box model errors only show up visually, and only at the end.

**Mitigation:** the phase order (§5) is **pixel-first**. P1 is the thinnest
vertical path (trivial parse → minimal block layout → real paint with a REAL
galley) wired end-to-end. Each subsequent P (P2 color, P3 box, P4 inline)
renders something new and visible. The mock measurer only serves deterministic
geometry tests (P3), never as a progress proxy.

### Risk 3 — Full reflow per frame + no cache (text dominates the time)

**The danger:** immediate mode re-paints everything; the ephemeral `StyledNode<'a>` tied
to the frame makes caching harder; if TS calls `egui.html` every frame,
we re-parse+re-measure everything and galley measurement dominates.

**Mitigation:** P6 dedicated to caching. Hash of HTML+CSS → reuse `DisplayList` +
galleys (owned via `Arc`) when the string does not change; rebuild the tree only on
change. The cache stores the **owners**, not the borrowed tree — the ephemeral
lifetimes stay confined to the construction step. Culling via `show_viewport`
(P5) cuts the paint of off-screen items. DPI changes → recreate galleys.

### Risk 4 — Hit-testing and event identity across frames

**The danger:** `allocate_painter` gives one `Response` for the whole screen; "which box
was clicked" is on us. A node-id generated by parse order is as fragile as an
index when the string changes between frames. `:hover` with 1 frame of delay flickers.

**Mitigation:** in-house hit-testing (§3) — `hit_rects` in the display list, tested in
**reverse order** (top-most), matching by `node_id`. Stable identity across
frames via **explicit** `key`/`id` in the HTML (no differential reconciliation in the
MVP — we do not promise what we do not have). `:hover` **cut** from the MVP (§0.2);
when it enters, it is re-layout on the next frame with the pointer known.

### Risk 5 — Unit resolution at the wrong moment (`%` too early)

**The danger:** `enum Dimension { Auto, Px(f32) }` discards `%` in the cascade, but
`%`/`auto` of width/margin/padding resolve against the **containing block** at
layout, not against the parent's computed.

**Mitigation:** `Dimension { Auto, Px(f32), Percent(f32) }` — `%` and `auto`
**survive** until Phase 4 and resolve there. `em`/`rem` resolve early (Phase 3,
against the parent's `font-size`). The two resolution moments are distinct by
design (§1, §3, §5 P2/P3). The whole spec was written with that separation.

> **Cross-cutting risk (fonts):** egui resolves **one** embedded family. There is
> no `font-family`/fallback/web fonts/arbitrary weight synthesis for free —
> all cut (§0.2). `font-weight`/`font-style` map to what egui
> offers in the single family (embedded bold/italic); arbitrary weights are not
> promised.

---

## 7) What we reuse vs rewrite

| Current artifact | Decision | Detail |
|---|---|---|
| **`html.rs::tokenize` / `Parser{pos,input}`** | **REUSE, evolve** | char-by-char + entities + unknown-tag tolerance migrate to `rts-html/src/dom/parser.rs`, but from "disposable token stream" to "recursive-descent that rebuilds nesting". Unknown tag becomes `Element` (not discarded — discarding loses the subtree). |
| **`html.rs::parse_html_to_cmds`** | **REWRITE / delete at the end** | the "2-flag bold/italic stack flattened at parse time" logic dies — it is exactly the failure (no inheritance/cascade/ancestry). Replaced by the 5-tree pipeline. Deleted in P7 after parity. |
| **`egui.html(string)` (ABI `__RTS_FN_NS_EGUI_HTML`)** | **REUSE signature, swap body** | the entry point and the ABI signature do not change. The body goes from `→ Vec<WidgetCmd>` to `→ rts_html::render(html, css, &measurer) → DisplayList` stored in the `UiCtx`. **Zero TS change.** |
| **`WidgetCmd` + `frame.rs::drenar` (simple mode)** | **REUSE untouched** | `egui.label/button/slider`, `horizontalBegin/End`, index-based pairing, recursive drain — remains the "simple mode". It is immediate and egui does it well. The calculator and current widgets do NOT break. |
| **`WidgetCmd` in HTML mode** | **REPLACE with DisplayList (do NOT convert)** | `egui.html` does NOT emit `WidgetCmd`. Converting HTML→`WidgetCmd` would reintroduce the flat queue (no ancestry/inheritance/cascade) under another name — the exact failure we eliminated. HTML mode is a **new and parallel** path: `DisplayList` consumed by an absolute-Painter walker, not by `drenar`. |
| **`UiCtx`** | **EXTEND** | gains `FrameContent { Simple(Vec<WidgetCmd>) | Html(DisplayList) }`; `endFrame` picks the walker by content. HTML click events matched by `node_id`/`key` (not by index). |
| **`ctx.rs` index-based buttons (`button_cursor`)** | **REUSE pattern, swap key** | the 1-frame-latency pattern (previous frame's result) is reused for HTML clicks, but matched by `node_id`/`key` instead of positional index (more stable when the tree changes). |
| **Window backend (`app.rs`, wgpu, event loop)** | **REUSE untouched** | window/surface/input loop via eframe remain. |

**Simple+HTML composition in the same frame** (`egui.label()` + `egui.html()`
together): out of the MVP. `endFrame` assumes exclusivity per frame. Composing
two walkers in the same window with correct ordering is defined in a later phase, not
promised now.

---

## Appendix A — Synthesis of the decision

| Axis | Today (flat queue) | Target (tree engine) |
|---|---|---|
| Structure | `Vec<WidgetCmd>`, index-based pairing | 5 trees: DOM→Style→Layout→DisplayList→Paint |
| Style | `{bold, italic}` flattened at parse | cascade + specificity + inheritance (`ComputedStyle`) |
| Position | egui stacks (`ui.horizontal`) | own box model computes `x,y,w,h` |
| Text | `RichText` (egui positions) | `LayoutJob` (egui measures+wraps) + `Painter::galley` (we position) |
| `%`/`auto` | n/a | resolved at **layout** (containing block), not in the cascade |
| egui | layout (widgets) | absolute paint + measurement + atlas + scroll + (us: hit-testing) |
| Location | `rts-egui` (parser+render together) | `rts-html` (tree, pure Rust) + `rts-egui` (window+paint) |
| First pixel | — | **P1 (week 1)**, thin vertical path, not at the end |
| `WidgetCmd` | only path | survives in simple mode; replaced by DisplayList in HTML mode |
| `egui.html(str)` | → `Vec<WidgetCmd>` | → `DisplayList` (same ABI, swapped body) |
| Scope | implicit | **static block+inline, LTR, single font** — flex/grid/position-abs/animations CUT |

The 5-tree architecture is right (it is the canonical pipeline). What the critique
corrected, and this plan incorporates: the scope is honest and cut, egui does the
text, `%` resolves at layout, there is a pixel in the first week, and there is a cache and
node-identity plan **before** writing Phase 4. Having the plumbing ready is not
having an engine — the work is the content of each box, and the most expensive content
(text) was placed at the heart (P4), not at the end.
