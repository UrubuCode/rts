# DOM in TS + dumb egui canvas — architecture (agreed 2026-06-25)

> Decision made in this session with the user, incorporating the other dev's vision.
> API fidelity reference: **MDN — Document Object Model**
> (https://developer.mozilla.org/en-US/docs/Web/API/Document_Object_Model).
> Replaces the previous "egui does the layout" design from the F0–F5 roadmap from the
> point where the layout migrates to TS (see "Relation to the roadmap" at the end).

## Why the DOM (and the layout) in TS

It's not ergonomics — it's **parallelism**. RTS has silent parallelization (passes
that rewrite TS code to run on rayon automatically). If the **DOM layout
computation is TS code**, the RTS parallelizer reaches it: layout of N independent
nodes becomes parallel for free. A layout engine in **Rust** (compiled)
stays out of reach of those passes. Therefore, the layout lives in TS on purpose.

## The division of responsibilities (3 layers)

```
rts-dom (Rust)          the TREE + parser + state. rts:dom ABI to READ the nodes.
   ↓ (TS reads via ABI)
DOM facade (TS)         Node/Document/Element/Text/NodeList (MDN spec) + the LAYOUT
   ↓ (commands)         (computed in TS → parallelizable). Emits paint commands.
rts-egui (dumb canvas)  drawRect/drawText/drawLine + measureText. ONLY executes + measures.
```

### Layer 1 — `rts-dom` (Rust): the tree, not the layout

Stays in Rust for parsing performance and because it is already done/tested:
- HTML parser → arena tree, versioned `NodeId` `{gen,idx}`, O(1) indices.
- store of `Dom`s per handle (`crate::store`, single source of truth).
- READ ABI `rts:dom` for the tree that TS consumes to do layout:
  `parseHtml`, `querySelector`/`querySelectorAllCount`/`At`, `childCount`/`At`,
  `getText`, `getAttribute`, `tagName`, `setText`/`setAttr`/`createElement`/
  `appendChild`/`removeNode`, `rootId`. **No layout computation here.**

### Layer 2 — DOM facade in TS (prelude): the spec + the layout

The `.ts` facade (today `crates/rts-dom/src/dom.ts`) implements the DOM API
**faithful to MDN** over layer 1's ABI, and — the new point — **computes the layout in TS**:
- Interfaces (MDN subset, see table below): `Node`, `Document`, `Element`,
  `Text`, `NodeList`/`HTMLCollection`.
- The **layout engine in TS**: walks the tree, computes positions/sizes of each
  box (box model: margin/padding/border/width), resolves `width%` against the parent,
  and for text uses `measureText` (layer 3). This traversal is what the RTS
  parallelizer can accelerate.
- Emits the list of **paint commands** for layer 3.

### Layer 3 — `rts-egui` (dumb canvas): only paints + measures text

egui stops doing layout. It becomes a canvas of primitives:
- `drawRect(x,y,w,h, fillRGBA, strokeW, strokeRGBA, radius)`
- `drawText(x,y, s, RGBA, size, bold, italic, mono)`
- `drawLine(x1,y1,x2,y2, RGBA, w)`
- `measureText(s, size, bold) -> width` (and line height) — **the only
  "intelligent" thing that remains**, because measuring text requires the font
  (egui's atlas). TS does NOT measure text (Risk 1 of the roadmap: never
  reimplement `glyph_width`).
- the loop: TS computes layout → sends commands → egui paints the frame.

## Why egui does NOT disappear entirely (the text wall)

The pure version "egui only paints ready-made rectangles, TS does 100%" is
**impossible today**: to compute where a line breaks, TS would need the font's
metrics (width of each glyph), which live in egui/wgpu. Reimplementing font
metrics in TS is big and slow (Risk 1). So egui retains EXACTLY one
intelligent responsibility: **measuring text** (`measureText`). All the rest of
the layout is TS.

## DOM subset to implement (faithful to MDN, phases)

| Interface | Members (phase 1 = minimum viable) |
|---|---|
| `Node` | `nodeType`, `nodeName`, `parentNode`, `childNodes`, `firstChild`, `nextSibling`, `textContent` (get/set), `appendChild`, `removeChild`, `insertBefore` |
| `Document` | `documentElement`, `body`, `querySelector`, `querySelectorAll`, `getElementById`, `createElement`, `createTextNode` |
| `Element` | `tagName`, `id`, `className`, `classList`, `textContent`, `innerHTML`, `attributes`, `children`, `getAttribute`/`setAttribute`/`removeAttribute`/`hasAttribute`, `querySelector`/`querySelectorAll`, `appendChild`/`removeChild`/`insertBefore`, `remove` |
| `Text` | `data` |
| `NodeList`/`HTMLCollection` | `length`, access by index |

Already implemented (phase 0, `dom.ts`): `Document` (querySelector/querySelectorAll/
getElementById/createElement/documentElement) + `Element` (textContent get/set,
tagName, id, className, getAttribute/setAttribute/hasAttribute, querySelector/
querySelectorAll, children, appendChild, remove). Validated E2E.

**Design rules imposed by the engine (raised empirically):**
1. Every public property = getter/setter, never a public field read post-call.
2. `T | null` APIs (querySelector) = class METHODS, never free functions.

## Implementation plan (slices)

1. **F-canvas:** egui exposes `drawRect`/`drawText`/`drawLine`/`measureText`
   (replaces/coexists with `egui.html`). Loop: TS sends commands.
2. **F-layout-TS:** minimal layout engine in TS (vertical blocks + text via
   `measureText`) emitting commands. Replaces the `render.rs` that did layout.
3. **F-dom-spec:** complete the facade to the MDN table above (Node/Text/classList/
   innerHTML/attributes/insertBefore).
4. **F-parallel:** validate that the RTS parallelizer picks up the layout loop.

## Relation to the F0–F5 roadmap (`docs/specs/html-engine/`)

The roadmap said "egui does the layout by default". This decision **diverges** from
the moment the layout migrates to TS (motivated by parallelism, which the roadmap
did not consider). `rts-dom` (tree/state) and the doctrine continue; what changes is
WHO computes the layout. Update the roadmap when this architecture stabilizes.

## What is NOT lost from the work already done

- `rts-dom` (tree, parser, NodeId, store, read ABI): **base of layer 1**.
- `document`/`Element` facade (`dom.ts`): **base of layer 2** (gains the layout).
- `style.rs`/`block.rs` migrated to rts-dom: the **state** the TS-layout reads.
- egui's `render.rs` (layout via native egui) is what will be **replaced** by the
  dumb canvas + TS-layout — but it serves as a reference for the expected behavior.
