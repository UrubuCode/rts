# Render and input interfaces — isolated DOM, pluggable backend

> Spec of the TWO interfaces that isolate the DOM/layout from ANY window backend.
> The DOM does not know egui; egui is just ONE backend that implements these
> interfaces. Decided with the user (2026-06-25). Complements
> `docs/specs/dom-in-ts-architecture.md`. Status: **spec** (phased implementation).

## The principle: two flows, both abstract

A UI has output (painting) and input (mouse/keyboard). Both cross the
DOM↔backend boundary, and both are abstracted for the same reason: swapping the backend (egui →
web → headless) must not touch the DOM/layout.

```
DOM/layout (TS)  ──render commands──►  backend   [OUTPUT]
DOM/layout (TS)  ◄──raw input (poll)──  backend   [INPUT]
```

TS NEVER names `egui`. It talks to generic `render.*` and `input.*`; the active
backend (today egui) implements those primitives. Another backend means swapping the
implementation, not the DOM.

## Interface 1 — RENDER (output). The DOM commands, the backend paints.

The TS-layout computes positions and emits ABSOLUTE primitives. The backend only executes.
Colors `0xRRGGBBAA` (number); coords/sizes in points (number).

| Primitive | Signature | Semantics |
|---|---|---|
| `render.beginFrame(target)` | `(target) -> void` | opens a paint frame on the target (window). |
| `render.rect` | `(target, x, y, w, h, fill, strokeW, stroke, radius) -> void` | filled rectangle + border + corners. |
| `render.text` | `(target, x, y, text, color, size, flags) -> void` | text at (x,y) top-left. flags 1=bold 2=italic 4=mono. |
| `render.line` | `(target, x1, y1, x2, y2, w, color) -> void` | line. |
| `render.measureText` | `(target, text, size, bold) -> width` | **text width in the real font**. The ONLY render op the layout NEEDS to consult (measuring requires the font; TS doesn't have it). Synchronous. |
| `render.endFrame(target)` | `(target) -> void` | closes + presents the frame. |

> Today implemented as `egui.drawRect/drawText/drawLine/measureText/beginFrame/
> endFrame` (F-canvas PoC). The isolation step is renaming/routing to a generic
> `render` namespace that egui satisfies — TS stops importing
> `rts:egui` and starts talking only `render`.

### Display list (optional, evolution)
Instead of calling `render.rect(...)` N times, the layout can produce a DISPLAY
LIST (array of commands) and hand it to the backend all at once. Advantage: the backend
doesn't even need to be called by TS — it reads the buffer (enables headless, serializing,
sending over the network, parallelizing the generation). It differs only in the delivery; the primitives are
the same. Decide when it becomes a bottleneck.

## Interface 2 — INPUT (input). The backend captures raw, the DOM interprets.

**Who captures:** the backend (it has the window; the OS delivers input to it). It only
reports the RAW — position, click, key — WITHOUT knowing about DOM nodes.
**Who interprets:** the DOM/layout. It did the hit-test (it has the positions!), so
IT knows which node is under the mouse and dispatches the DOM events.

**POLLING** model (not reactive callback — the engine does not support capturing
closures well; roadmap F3): each frame TS asks the backend for the input state
and does the dispatch.

| Primitive | Signature | Semantics |
|---|---|---|
| `input.mouseX/mouseY` | `(target) -> number` | cursor position (points), in the render coord space. |
| `input.mouseDown` | `(target, button) -> bool` | button pressed NOW (0=left 1=right 2=middle). |
| `input.mouseClicked` | `(target, button) -> bool` | a full click happened THIS frame. |
| `input.wheel` | `(target) -> number` | scroll delta of the frame. |
| `input.keyDown` | `(target, keycode) -> bool` | key pressed now. |
| `input.keyPressed` | `(target, keycode) -> bool` | key fired this frame (with repeat). |
| `input.textInput` | `(target) -> string` | text typed this frame (UTF-8). |

### Hit-test and events: the DOM's (TS) responsibility
The DOM/layout, each frame:
1. reads `input.mouseX/Y` + `input.mouseClicked`.
2. **hit-test**: finds the node whose layout rectangle contains (x,y) — using the
   positions IT computed (stored in the layout pass). The backend does not participate.
3. dispatches the node's DOM events: `onclick`, `onmouseover`, `addEventListener`
   (all resolved in TS by polling; without storing capturing closures —
   state in module-level variables, the roadmap F3 pattern).

This way the backend stays DUMB (knows no nodes, does no hit-test) and the DOM
owns the event semantics — mirroring the browser (the compositor reports
coordinates; the DOM dispatches events).

## Why this division (summary)

- **Render:** the backend does not decide layout (TS decides) — it only paints primitives +
  measures text (needs the font). Swapping backends = reimplementing 6 primitives.
- **Input:** the backend does not interpret (the DOM does hit-test + events) — it only reports
  the raw. Swapping backends = reimplementing ~7 state reads.
- **Isolated DOM:** talks `render.*`/`input.*`, never `egui.*`. It's the "isolated system
  any render knows how to render" — and from which any backend knows how to read input.

## Implementation plan (phases)

1. **I0 — generic `render` namespace:** route the current `egui.draw*`/`measureText`
   to a `render` namespace (egui is the impl). The TS-layout starts talking
   `render.*`. (Isolation of the OUTPUT — the PoC already has the primitives.)
2. **I1 — `input` namespace:** egui exposes `input.mouseX/Y/clicked/...` via polling.
3. **I2 — hit-test in the TS-layout:** the layout stores the rectangles per node; a
   `hitTest(x,y)->node` in TS; `onclick` dispatch via polling.
4. **I3 — headless backend (proof of the isolation):** a second backend that
   implements `render.*` by writing to a PPM (screenshot) — proves the DOM
   renders without egui. (Optional, but it is the definitive test of the isolation.)
