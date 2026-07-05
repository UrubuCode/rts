# Target architecture: RTS HTML+CSS engine on top of egui

Confirmed. The `WidgetCmd` is a flat queue, index-paired, inline style with 2 flags (bold/italic), no ancestry. I have everything for the proposal.

---

# Target architecture: RTS's own HTML+CSS engine on top of egui

## 0) Thesis and up-front decision verdict

The current flat queue (`Vec<WidgetCmd>` in `ctx.rs:82`, paired by index, with inline style reduced to `{bold, italic}`) is a **token-stream renderer**, not an engine. It dies exactly where the research says a tree is needed: ancestry (containing block), inheritance (`color`/`font` from the parent), and cascade (specificity among competing rules). None of these three is decidable over a list with no parental topology.

The target architecture reproduces the canonical browser pipeline (web.dev/Servo/robinson) in **five chained trees** that only flatten at the end:

```
HTML bytes → DOM tree → (+ Stylesheet) → Style tree → Layout tree (com x,y,w,h) → Display list → Paint
   parser      árvore       cascade+        computed       box model              lista plana    egui Painter
              de nós       herança         + cascade        constraint solving                   absoluto
```

Decision on `WidgetCmd` (announced now, justified in §6): **the flat queue SURVIVES as the "simple mode"** (`egui.label/button/slider`, immediate, index pairing — what `frame.rs::drenar` already does and what egui does well). The **HTML mode is a NEW, separate path** that does NOT produce `WidgetCmd` — it produces a **Display list** consumed by an absolute Painter walker. The two coexist; there is no HTML→`WidgetCmd` conversion (that would perpetuate the flat queue under another name).

Location decision (§4): everything that is tree (DOM/CSS/Style/Layout/Display list) lives in a **new crate `rts-html`, pure Rust, zero dependency on egui/winit/wgpu**. `rts-egui` becomes the **window + paint backend** (consumes the display list and measures text). The high level is TS via the already-existing primitive `egui.html(string)`.

---

## 1) The phases of OUR engine — what each one produces and the Rust struct

Each phase is a module inside `rts-html` (500-line/file ceiling → each phase is a folder with `mod.rs` + submodules). Sketch of the structs (idiomatic Rust, based on robinson adapted to our constraints):

### Phase 1 — HTML → DOM tree (`rts-html/src/dom/`)

Produces: the content tree. It is the direct evolution of the `tokenize` in `html.rs:38` (which today throws tokens away in a stream); now the recursive-descent parser rebuilds the real nesting.

```rust
// dom/node.rs
pub struct DomNode {
    pub node_type: NodeType,
    pub children: Vec<DomNode>,
}

pub enum NodeType {
    Element(ElementData),
    Text(String),
}

pub struct ElementData {
    pub tag_name: String,
    pub id: Option<String>,            // extraído de attributes["id"], cache p/ matching O(1)
    pub classes: Vec<String>,          // split de attributes["class"], idem
    pub attributes: HashMap<String, String>,
}
```

The parser (`dom/parser.rs`) is recursive-descent over the same char-by-char `Parser { pos, input }` we already have; the difference is that `<p><b>x</b></p>` produces `Element(p) → [Element(b) → [Text("x")]]` instead of 3 loose tokens. An unknown tag becomes a generic `Element` (it is not discarded like today — discarding loses the subtree). Entities (`&amp;`/`&lt;`/`&gt;`) remain decoded in the `Text` node.

### Phase 2 — CSS parse → Stylesheet (`rts-html/src/css/`)

Produces: the parsed, indexable stylesheet. Subset of the css-subset research Phase 1 (O(1) selectors: tag/class/id/`*`/compound/list).

```rust
// css/stylesheet.rs
pub struct Stylesheet { pub rules: Vec<Rule> }

pub struct Rule {
    pub selectors: Vec<Selector>,       // lista separada por vírgula, já ordenada por especificidade desc
    pub declarations: Vec<Declaration>,
}

pub enum Selector { Simple(SimpleSelector) }   // P1: só simples; combinadores entram na P2

pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class: Vec<String>,
}

pub struct Declaration { pub name: String, pub value: Value }

pub enum Value {
    Keyword(String),                    // "block", "bold", "left"
    Length(f32, Unit),                  // 16.0 Px
    Color(Color),                       // #rrggbb → rgba
}
pub enum Unit { Px, Em, Percent }
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }
```

`Selector::specificity() -> (u32,u32,u32)` (IDs, classes, types), used to sort within `selectors` at parse time — exactly what robinson does to speed up matching.

### Phase 3 — Style tree (computed values, cascade + inheritance) (`rts-html/src/style/`)

Consumes DOM + Stylesheet. Produces: the mirror tree of the DOM where each node carries its **computed values** already resolved by the cascade and by inheritance. **This is where the tree is indispensable**: inheritance copies from the parent, and selectors/`em`/`%` resolve against the ancestor.

```rust
// style/styled_node.rs
pub struct StyledNode<'a> {
    pub node: &'a DomNode,                 // empréstimo do DOM (vida da árvore-pai)
    pub specified: PropertyMap,            // saída da cascata para ESTE nó
    pub computed: ComputedStyle,           // valores resolvidos (herança + unidades pré-layout)
    pub children: Vec<StyledNode<'a>>,
}

pub type PropertyMap = HashMap<String, Value>;

// computed: o que layout/paint leem sem reinterpretar strings
pub struct ComputedStyle {
    pub display: Display,                  // Block | Inline | None
    pub color: Color,                      // herdado se ausente
    pub font_size: f32, pub font_weight: u16, pub italic: bool,  // herdados
    pub text_align: TextAlign,             // herdado
    // box (non-inherited, default = initial):
    pub width: Dimension, pub height: Dimension,
    pub margin: Edges, pub padding: Edges, pub border: Edges,
    pub border_color: Color, pub background: Option<Color>,
}
pub enum Display { Block, Inline, None }
pub enum Dimension { Auto, Px(f32) }       // Em/Percent já resolvidos contra o pai aqui
```

Algorithm (`style/cascade.rs`): for each node, `matching_rules` collects `(specificity, &Declaration)` from the rules whose selector matches (O(1) match on the node itself: AND of tag/id/classes); sorts by ascending specificity; applies in order (more specific overwrites); tie → source order. Then `resolve_inherited` copies from `parent.computed` the missing inherited properties (text), and resolves `Em`/`Percent` against the parent's computed (`1.5em` with a 16px parent → 24px). `display:none` marks the node to be **excluded** from the next phase (it never becomes a box). Top-down recursion passing the parent's `&ComputedStyle`.

### Phase 4 — Layout tree (box model, x,y positions) (`rts-html/src/layout/`)

Consumes Style tree + viewport. Produces: the tree of boxes with **resolved geometry** (`x,y,w,h` per box, plus the four box-model areas).

```rust
// layout/box.rs
pub struct LayoutBox<'a> {
    pub dimensions: Dimensions,
    pub box_type: BoxType<'a>,
    pub children: Vec<LayoutBox<'a>>,
}

pub enum BoxType<'a> {
    Block(&'a StyledNode<'a>),
    Inline(&'a StyledNode<'a>),
    Anonymous,                              // embrulha inlines mistos sob um pai block
}

pub struct Dimensions {
    pub content: Rect,                      // x,y,width,height do content
    pub padding: EdgeSizes,
    pub border:  EdgeSizes,
    pub margin:  EdgeSizes,
}
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
pub struct EdgeSizes { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32 }
```

Block algorithm (`layout/block.rs`, robinson Part 6): **a single traversal**, top-down for widths/positions, bottom-up for heights:
1. **Widths (top-down)**: resolved by constraint — sums margin+border+padding+width; computes underflow; `width:auto` expands to fill the parent; `auto` margin absorbs. A child needs the parent's width → hence top-down.
2. **Position (x,y)**: the box goes below the previous ones in the container (`y = content.y_acumulado + margens/border/padding`).
3. **Heights (bottom-up)**: after laying out children, the parent's height = sum of the children's heights (unless `height` is explicit).

**Inline** is where `rts-egui` comes in as a **text measurement** service (§3): the width of a text run is not computable in pure Rust — it needs the atlas/shaping. Inline layout requests the measurement via an abstract trait (see §4, `TextMeasurer`), keeping `rts-html` decoupled from egui.

### Phase 5 — Display list (`rts-html/src/paint/display.rs`)

Consumes Layout tree. Produces: the **flat, ordered list** of drawing commands in absolute coordinates — the point where the tree finally flattens, and only now because ancestry/inheritance/constraint have already been resolved.

```rust
// paint/display.rs
pub enum DisplayItem {
    SolidRect { rect: Rect, color: Color },                 // background de bloco
    Border    { rect: Rect, edges: EdgeSizes, color: Color },
    Text      { x: f32, y: f32, run: TextRun },             // run já medido (galley-ready)
    Image     { rect: Rect, src: String },
}
pub struct TextRun {
    pub text: String,
    pub font_size: f32, pub weight: u16, pub italic: bool,
    pub color: Color,
    // opcional: handle do Galley já produzido na medição, p/ não re-shapear no paint
}
pub struct DisplayList(pub Vec<DisplayItem>);
```

Built by walking the Layout tree in stacking order (background → border → content → children). The order of the `Vec` **is** the z-order (the egui-as-paint research confirms: Painter draws back-to-front in call order).

### Phase 6 — Paint (absolute egui Painter) (`rts-egui`, NOT in `rts-html`)

Consumes the Display list. Produces: pixels. A walker in `rts-egui` translates each `DisplayItem` into an `egui::Painter` call, adding the surface origin:
- `SolidRect` → `painter.rect_filled(rect, CornerRadius::ZERO, color)`
- `Border` → `painter.rect_stroke(rect, ZERO, stroke, StrokeKind::Inside)`
- `Text` → `painter.galley(pos, galley, color)` (the galley comes from the Phase 4 measurement)
- `overflow` clipping → `painter.with_clip_rect(rect)`

---

## 2) Why a TREE and not the current flat queue

The `Vec<WidgetCmd>` queue (ctx.rs:82) has three structural flaws, each requiring parental topology a list does not have:

1. **No ancestry → no containing block.** Normal-flow layout is top-down for widths (the child inherits the available width from the parent) and bottom-up for heights (the parent sums the children's heights). That is parent↔child navigation. In the queue, `ParagraphBegin/End` fakes nesting via **index pairing** — it works for a stacked `ui.horizontal`, but there is no way to ask "what is the width of my containing block?" because the parent does not exist as a node, only as a positional marker already consumed.

2. **No inheritance.** Today the inline style is a stack of 2 flags (`Style{bold,italic}` in html.rs:24) rebuilt at each token. There is no way for `color`/`font-size`/`text-align` to flow from a `<div>` to grandchild paragraphs: the queue does not store "the parent was blue". Inheritance is literally copying from `parent.computed` — with no parent node, it is impossible. That is why `InlineText` carries bold/italic embedded: the parser had to "flatten" the inheritance at emission time, losing the ability for any later CSS rule to override.

3. **No cascade.** There is no CSS in the queue — the style comes only from inline tags. Cascade requires collecting N competing rules matching a node, sorting them by specificity (IDs, classes, types) and order, and letting the strongest win. That presupposes (a) an identifiable node with tag/id/classes and (b) ancestors for descendant selectors. The flat queue has neither node nor ancestor — only already-styled text.

The tree solves all three because it keeps the parental relationship alive from Phase 1 through Phase 4. It only "flattens" in Phase 5 (display list), **after** inheritance, cascade and constraint-solving have already consumed the topology — exactly the order web.dev/Servo describe.

---

## 3) egui as PAINT, not LAYOUT — where we stop stacking and start computing x,y

Today `frame.rs::drenar` (lines 215-289) does egui-as-layout: it calls `ui.label()`/`ui.button()`/`ui.heading()` and lets egui stack via `ui.horizontal`/`ui.horizontal_wrapped`. egui decides the positions. That is correct for the **simple mode** and continues.

In **HTML mode**, we stop stacking the instant an own box model exists: the engine computes each box's `x,y,w,h` (Phase 4) and `rts-egui` stops calling layout widgets and starts **painting at absolute coordinates**. The concrete transition:

- **Get the surface**: `let (response, painter) = ui.allocate_painter(desired_size, Sense::hover())`. `response.rect.min` is the `(0,0)` origin of our box model — we add it to each `(x,y)` of the display list.
- **Do NOT use** `ui.horizontal/vertical/Grid/Frame` nor `RichText` on the content: they position for us and fight the box model. We use only `allocate_painter` + `Painter` + `ScrollArea::show_viewport`.
- **egui becomes four services, all the ones we don't want to rewrite:**
  1. **Text measurement + line-breaking**: `ctx.fonts(|f| f.layout_no_wrap(text, font_id, color))` for run width, or `glyph_width`/`row_height` for our own line-breaking algorithm in Phase 4 inline. For inline spans with different fonts/colors (`<b>`, `<span style>`), we assemble a `LayoutJob` with a `LayoutSection`/`TextFormat` per span and measure with `layout_job`.
  2. **Font atlas + rasterization**: managed by egui/epaint; the `Galley` produced during measurement references the atlas and is consumed directly at paint via `painter.galley(pos, galley, color)`.
  3. **Absolute Painter**: `rect_filled`/`rect_stroke`/`galley`/`line`/`image` on screen `Pos2`/`Rect`. No transformation, absolute logical coordinates.
  4. **ScrollArea with virtual viewport**: `ScrollArea::vertical().show_viewport(ui, |ui, viewport: Rect| { ... })`. The `viewport` (relative to the content, `min==ZERO` at the top) gives **culling for free** — we paint only the `DisplayItem`s that intersect the viewport, which is essential in immediate mode (egui repaints everything every frame, no cache). We allocate the total size via `ui.allocate_space(content_size)` so the scrollbar sizes correctly; content→screen translation = `screen = content + (ui.min_rect().min - viewport.min)`.

Golden rule: **egui never sees the tree.** It only receives already-computed `Rect`/`Pos2`/`Galley` and measures text when asked. Z-index is the display list's iteration order (or `ctx.layer_painter` per stacking context). DPI: recreate `Galley`s when `ctx.pixels_per_point()` changes.

---

## 4) Where each layer lives

| Layer | Crate | Language | Responsibility | Depends on egui? |
|---|---|---|---|---|
| High-level API `html(str)`, stylesheet, handlers | `rts-shared/src/stdlib/*.ts` (or builtin TS) | TS | ergonomic surface over the `egui.html` primitive | no |
| **DOM + CSS + Style + Layout + Display list** | **`rts-html` (NEW)** | Pure Rust | Phases 1–5; tree model; constraint solving | **NO** (zero egui/winit/wgpu dep) |
| Window backend + paint + measurement | `rts-egui` | Rust | Phase 6: DisplayList→Painter walker; provides `TextMeasurer`; ScrollArea; event loop; wgpu | yes |
| ABI primitive `egui.html(ptr,len)` | `rts-egui` extern "C" | Rust | entry point: receives string, calls `rts-html`, stores display list in `UiCtx` | yes |

Key decoupling point: Phase 4 (inline layout) needs to measure text, which only egui does well — but `rts-html` **cannot** depend on egui (otherwise it becomes another UI crate and loses reuse/testing). This is solved with a **dependency-inversion trait** defined in `rts-html` and implemented in `rts-egui`:

```rust
// em rts-html: a abstração, sem nada de egui
pub trait TextMeasurer {
    fn measure(&self, text: &str, font_size: f32, weight: u16, italic: bool) -> (f32, f32); // (w,h)
    fn break_line(&self, text: &str, font_size: f32, max_width: f32) -> Vec<LineBreak>;
}
// layout() recebe `&dyn TextMeasurer`; em rts-egui, o impl chama ctx.fonts(|f| f.layout_*)
```

Thus `rts-html` is testable in isolation (with a mock measurer) and could, in the future, have another paint backend. Each phase is a folder (`dom/`, `css/`, `style/`, `layout/`, `paint/`) with `mod.rs` + submodules, respecting the project's 500-line/file ceiling.

On the PRIMORDIAL-vs-Registry doctrine: HTML/CSS **have no native syntax** in JS/TS, therefore they CANNOT be named in `rts-codegen-new`. They live in Rust as **render primitives** exposed via ABI namespace (`egui.html` is a `NamespaceMember` in `SPECS`, just like `io.print`). The high-level logic (assembling the HTML string, reacting to events) is TS. The codegen engine never mentions "html" — it only resolves a generic namespace call. None of this crosses the codegen: it is the same boundary as `io`/`fs`/`ui`.

---

## 5) What we reuse from what we already have

- **`html.rs` evolves to produce a DOM tree.** The `tokenize` (lines 38-82) and the char-by-char `Parser{pos,input}` are the base; they change from "throwaway token stream" to "recursive-descent parser that rebuilds the nesting". Entity decoding and unknown-tag tolerance migrate intact (but an unknown tag becomes a generic `Element`, not a discard — discarding loses the subtree). This file migrates from `rts-egui` to `rts-html/src/dom/parser.rs`.
- **The `egui.html(string)` primitive remains the entry point.** The ABI signature (`__RTS_FN_NS_EGUI_HTML(h, ptr, len)`) does not change. What changes is the body: today it calls `parse_html_to_cmds` → `Vec<WidgetCmd>`; it will call `rts_html::render(html_str, stylesheet, &measurer)` → `DisplayList`, stored in the `UiCtx` for the frame's paint walker. Zero change in TS.
- **The `WidgetCmd` queue SURVIVES — decision made: coexistence, not replacement.**
  - **Simple mode** (`egui.label/button/slider`, `egui.horizontalBegin/End`): keeps `Vec<WidgetCmd>` + `frame.rs::drenar` + index pairing. It is immediate, it is what egui does well, and there is no reason to force HTML where the dev just wants a button. Untouched.
  - **HTML mode**: NEW, parallel path. `egui.html` does NOT emit `WidgetCmd` — it emits a `DisplayList`, consumed by an absolute Painter walker (not by `drenar`). Converting HTML→`WidgetCmd` would reintroduce the flat queue (no ancestry/inheritance/cascade) under another name — the exact flaw we are eliminating.
  - The `UiCtx` gains a second frame buffer: besides `cmds: Vec<WidgetCmd>`, a `display_list: Option<DisplayList>` (or an enum `FrameContent { Simple(Vec<WidgetCmd>), Html(DisplayList) }`). `endFrame` picks the walker by the content present.
  - Interactivity in HTML mode (click on `<button>`, scroll): the paint walker, when emitting a clickable item, registers its `Rect` and consults the `response` of `allocate_painter`/`ScrollArea` for hit-testing — the same 1-frame-latency pattern `button_results`/`button_cursor` already use, but keyed by **DOM node-id** instead of positional index (more stable; the index breaks when the tree changes between frames).

Summary of the §5 decision: `html.rs` → `rts-html/src/dom/`; `egui.html(string)` remains as the entry point (body swapped); `WidgetCmd` remains for the simple mode and is **replaced by the display list in HTML mode** (not converted).

---

## 6) Suggested implementation sequence (phaseable, each step green)

Mirrors the recommendation of the css-subset research (mentally clone robinson in Phase 1) and respects "resolve blocker before moving on":

1. **`rts-html` + DOM tree** — create the crate, migrate `tokenize`, recursive parser → `DomNode`. Isolated unit test (no egui). `[▰▰▱▱▱▱▱▱▱▱] 20%`
2. **CSS parser + Stylesheet** — Phase 1 subset (tag/class/id/`*`/compound, color/length/keyword declarations). `[▰▰▰▱▱▱▱▱▱▱] 30%`
3. **Style tree** — O(1) matching + specificity + inheritance → `StyledNode`/`ComputedStyle`. `[▰▰▰▰▱▱▱▱▱▱] 45%`
4. **Block layout** — top-down width / bottom-up height; mocked `TextMeasurer`. `[▰▰▰▰▰▰▱▱▱▱] 60%`
5. **Display list** — walk of the Layout tree → `Vec<DisplayItem>`. `[▰▰▰▰▰▰▰▱▱▱] 70%`
6. **`rts-egui` backend** — real `TextMeasurer` via `ctx.fonts`; DisplayList→Painter walker; `allocate_painter` + `ScrollArea::show_viewport`; swap the body of `egui.html`. `[▰▰▰▰▰▰▰▰▰▱] 90%`
7. **Inline layout + hit-testing** — text runs measured via `LayoutJob`, line breaking, `<button>` click by node-id. `[▰▰▰▰▰▰▰▰▰▰] 100%`

---

## Synthesis

| Axis | Today (flat queue) | Target (tree engine) |
|---|---|---|
| Structure | `Vec<WidgetCmd>`, index pairing | 5 trees: DOM→Style→Layout→DisplayList |
| Style | `{bold,italic}` flattened at parse | cascade + specificity + inheritance (`ComputedStyle`) |
| Position | egui stacks (`ui.horizontal`) | own box model computes `x,y,w,h` |
| egui | layout (widgets) | paint (absolute Painter) + measurement + atlas + ScrollArea |
| Location | `rts-egui` (parser+render together) | `rts-html` (tree, pure Rust) + `rts-egui` (window+paint) |
| `WidgetCmd` | only path | survives for simple mode; replaced by DisplayList in HTML mode |
| `egui.html(str)` | → `Vec<WidgetCmd>` | → `DisplayList` (same ABI signature, body swapped) |

The tree persists from Phase 1 through Phase 4 because ancestry, inheritance and constraint-solving require the parental topology; it only flattens in the display list, exactly when egui takes over — as paint, not as layout.
