# RTS UI Stack — CURRENT STATE (consolidated plan)

> Map of what exists today in the UI stack experiment, what is missing, and the
> relation to the egui roadmap (F0-F5). Dated 2026-06-25. Branch:
> `feat/dom-owns-state-and-facade`. Read together with:
> - `dom-in-ts-architecture.md` (DOM/layout in TS, dumb egui canvas)
> - `dom-render-input-interfaces.md` (abstract render/input interfaces)
> - `engine-limits-found-building-ui.md` (engine limits found in practice)

## The architecture in one figure

```
rts-dom (Rust)        tree + parser + state(style/layout-intent) + store. Read ABI.
   ↓ (TS reads via ABI)
DOM facade (TS)       Document/Element (MDN spec) + LAYOUT in TS (parallelizable)
   ↓
rts-render (Rust)     ABSTRACT interface render.* / input.* + trait Renderer/InputSource
   ↓ (active backend)
rts-egui              ONE backend that implements the traits (paints + captures input). Swappable.

   And, in parallel to the DOM, on the SAME base:
rts:canvas (TS)       immediate-mode UI (Canvas/App) + components + base loop. NO DOM.
```

The key: TS speaks `dom.*` / `render.*` / `input.*` — **never `egui.*`** (except
window/loop). egui is a pluggable backend; swapping it changes nothing above.

## ✅ What ALREADY EXISTS (delivered and validated on screen)

### New crates
- **`rts-dom`** — headless retained DOM: HTML parser, arena tree, versioned
  `NodeId` `{gen,idx}`, query (tag/#id/.class) O(1), mutation, public store
  (`with_dom`), style state (`style.rs`) and layout-intent (`block.rs`)
  migrated here. ABI `rts:dom` (parseHtml/querySelector/setText/getText/
  getAttribute/tagName/childCount/childAt/nodeStyleSlot/displayOf/defineStyle/...).
- **`rts-render`** — ABSTRACT interface: trait `Renderer` (rect/text/line/image/
  measureText/begin/end) + `InputSource` (mouse/key/text via polling) + registry
  of the active backend. ABI namespaces `render` and `input`. `.ts` facade `rts:canvas`
  (Canvas/App/components) as a prelude.

### DOM layer (TS)
- **`document`/`Element` facade** faithful to MDN: querySelector/querySelectorAll/
  getElementById/createElement/documentElement; textContent (get/set), tagName,
  id, className, getAttribute/setAttribute/hasAttribute, children, appendChild,
  remove. Validated: `document.querySelector(".t").textContent = x` works.
- **Layout engine in TS** (PoC): walks the tree via ABI, computes positions/box
  model, emits canvas commands. It is TS code → a target for the RTS parallelizer.

### render/input layer (abstract, pluggable backend)
- `render.*`: rect/text/line/**image** (RGBA bitmap → video/image/viewport)/
  measureText/beginFrame/endFrame. egui is the backend (impl of the trait).
- `input.*` — **COMPLETE** (3 phases, see `input-system-design.md`):
  - **Mouse:** mouseX/Y, down/clicked/**pressed/released/doubleClicked**,
    **deltaX/Y**, **dragging** (native drag), wheel/**wheelX**, **setCursor**.
  - **Keyboard:** full codes (A-Z 100-125, 0-9 130-139, F1-F12 140-151,
    editing/navigation 1-15) × **keyPressed/keyDown/keyReleased**; **modifiers**
    modCtrl/Shift/Alt/Cmd (shortcuts).
  - egui captures the raw input (winit/OS); `input.*` is the abstract facade; swappable for
    another backend. **Input does NOT depend on egui** — it is a plugin.
  - **Ergonomic layer (TS, in App):** real FOCUS (focusedId/setFocus/isFocused),
    `clickable(id)` (idle/hover/pressed/clicked, with release-inside), `textField(id)`
    (field with exclusive focus — only the focused one types). Solves real forms.

### Canvas layer (immediate-mode UI, no DOM)
- **`Canvas`/`App`** + `createApp`/`createAppAt`: base loop (the dev keeps the while;
  beginFrame/delta/endFrame remove the boilerplate), delta time (via `rts:time`),
  frameCount, **FPS controller** (setFps/fps).
- **Components**: label, panel, button, slider, checkbox, progressBar, **tabs**,
  textInput, **automatic layout** (column + auto*). Built-in hit-test/hover/click.

### Windows
- **multi-window** (N windows in one program — already supported, UiCtx per handle).
- **multi-monitor**: `moveWindow` + `setNextWindowPos`/`createAppAt` (born on the
  chosen monitor — reliable).

### Examples (all run, validated on screen)
claude-dom-headless / dom-facade / dom-interactive / canvas-poc / render-abstract /
input-abstract / layout-ts / canvas-facade / app-loop / components / tabs /
multiwindow / image-video / **showcase** (4 tabs) / **keyboard** (keyboard+mods) /
**mouse** (drag/double/cursor) / **focus-form** (2 fields with real focus).

### Docs
3 architecture specs + the map of engine limits.

## ⬜ What is MISSING (next steps, prioritized)

### Short term (refinement of what exists)
1. **EXACT text measurement** — today `measureText` is approximate (0.52·size·n); the
   exact one via egui's font atlas is an isolated TODO in `canvas.rs`/`measure_text`.
2. **Backspace/cursor editing in textField** — append + focus ALREADY work;
   backspace/selection/cursor-in-the-middle depend on the `.length`/string-ops limit over
   unproven shape (see limits #4).
3. **Vsync-off mode** (benchmark) — to measure FPS above the monitor's ceiling; today
   Fifo limits it (and there is a kill-gate because of the window-that-stopped bug).
4. **2nd backend (headless/PPM)** — actually prove "N generic renders" (render.*
   writing to a buffer/PNG, no window). It is the definitive test of the isolation.
5. **Input phase 4** (optional) — drag-helper in App + automatic cursor (hand
   over clickable). Touch/gamepad/IME = distant future.

> **INPUT is COMPLETE** (phases 1-3: keyboard+mods, rich mouse, focus+events) —
> it left the pending list. "Pointer data" bug (string return with the U64 alias
> instead of literal AbiType::Handle) fixed — see limits #9.

### Medium term (completing layers)
5. **DOM facade → full MDN spec** — Node/Text/classList/innerHTML/insertBefore/
   removeChild (see `dom-in-ts-architecture.md` table).
6. **Complete TS layout engine** — text wrap, width%, horizontal/grid display,
   margin-collapse; and hooking it up to the parallelizer.
7. **Rich DOM events** — addEventListener-like via polling; hover/focus.
8. **Image decoder (PNG/JPG)** and then video (codec) — `render.image` already
   accepts any RGBA bitmap; the source of the pixels is what's missing.

### ENGINE dependencies (unblock first — see limits doc)
Limits #1/#2/#4 of `engine-limits-found-building-ui.md` (const-capture-in-fn,
dispatch-over-getter, .length/string-ops over unproven shape) are the ones that most
block rich UI. Implementing them in the engine simplifies the facade and the
components A LOT (today full of workarounds: number-instead-of-bool, literals,
direct-methods).

## Relation to the egui roadmap F0-F5 (`html-engine/rts-html-roadmap.md`)

The experiment **diverged** from F0-F5 from the moment layout migrated
to TS (motivated by RTS parallelism — the roadmap assumed egui-layout). What
remains from the roadmap: `rts-dom` (tree/state) and the doctrine. What changed: who
does the layout (TS, not egui) and the abstract render/input interface (new). The
F0-F5 roadmap describes the "egui renders HTML directly" path; this experiment describes
"DOM/layout in TS over abstract render". Decide with the team which one is official —
or whether they coexist (the roadmap as pure-HTML-engine, this as general-UI-stack).

## One-line summary

Today there exists a complete and functional UI foundation on top of RTS — real DOM,
layout in TS, abstract render/input with pluggable backend, ergonomic canvas,
component library, multi-window/monitor, render.image — all validated on
screen, with a clear map of the engine limits to unblock. Ready to consolidate and
grow.
