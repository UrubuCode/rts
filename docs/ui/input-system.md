# Input system — spec of the complete target

> The current input (`rts:input`) is a MINIMAL raw polling (it proved the
> architecture, but has big holes for serious UI/browser). This spec designs the
> COMPLETE target before implementing — what the `InputSource` trait needs, what
> goes in TS, and the phases. Dated 2026-06-25. Complements `dom-render-input-interfaces.md`.

## What exists TODAY (`InputSource` / `input.*`)

| Fn | State |
|---|---|
| `mouseX/mouseY` | ✅ position |
| `mouseDown(button)` | ✅ holding (0=left 1=right 2=middle) |
| `mouseClicked(button)` | ✅ full click in the frame |
| `wheel` | ✅ vertical scroll |
| `keyPressed(key)` | ◐ only 8 keys (Enter/Esc/Space/Backspace/4 arrows) |
| `textInput` | ✅ typed text (UTF-8) |

**Holes:** keyboard almost empty; no modifiers; no separate press/release; no
drag/double-click; no focus; no cursor. Manual polling everywhere (no events).

## Principle (kept)

The backend CAPTURES the raw (it owns the window); the DOM/app INTERPRETS (hit-test + focus +
events). Polling, not callback (engine limit: capturing closures break).
Everything abstract — the TS speaks `input.*`, never `egui.*`. egui maps from its APIs
(`egui::Key`, `Modifiers`, `PointerState`).

## Layer 1 — RAW INPUT (`InputSource` trait + `input.*`)

### Mouse (complete it)
| New fn | Semantics |
|---|---|
| `mousePressed(button)` | button WAS pressed THIS frame (up→down transition) |
| `mouseReleased(button)` | button WAS released this frame (down→up) |
| `mouseDoubleClicked(button)` | double-click this frame |
| `mouseDeltaX/Y` | relative cursor movement in the frame (camera/scrub) |
| `wheelX` | horizontal scroll (we already have `wheel` = vertical) |
| `dragging(button)` | `true` while dragging (pressed + moving) — convenience |

### Keyboard (the biggest hole — complete NEUTRAL codes)
Today 8 keys. Target: a neutral table covering what egui exposes. `KEY_*` codes:
- **Editing/navigation:** Enter, Escape, Space, Backspace, Tab, Delete, Insert,
  Home, End, PageUp, PageDown, ArrowUp/Down/Left/Right.
- **Letters:** A..Z (codes 100..125).
- **Digits:** 0..9 (codes 130..139).
- **Function:** F1..F12 (codes 140..151).
- (Punctuation/symbols arrive via `textInput`, not as a keycode — follows egui.)

| New/expanded fn | Semantics |
|---|---|
| `keyDown(key)` | key held NOW (continuous state) |
| `keyPressed(key)` | fired this frame (with auto-repeat) |
| `keyReleased(key)` | released this frame |

### Modifiers (essential — shortcuts, shift+click)
| Fn | Semantics |
|---|---|
| `modCtrl()` | Ctrl held |
| `modShift()` | Shift |
| `modAlt()` | Alt |
| `modCmd()` | Cmd/Super (Win/⌘) — egui `command` (cross-platform) |

With this the TS does `if (input.modCtrl() && input.keyPressed(KEY_C))` → copy.

### Cursor (visual feedback)
| Fn | Semantics |
|---|---|
| `setCursor(target, kind)` | changes the cursor: 0=default 1=pointer(hand) 2=text(I) 3=resize... egui maps to `CursorIcon`. The app calls it when hovering over a link/field. |

## Layer 2 — EVENTS + FOCUS (in TS, over the raw input)

Raw polling is the base; for a large UI, a convenience layer in TS (in
`rts:canvas`/the DOM facade) builds what's missing — WITHOUT callback (state in
variables; the app calls it per frame):

- **FOCUS:** a `focusedId` (which element has the keyboard). A `clicked` on a field
  focuses it; Tab moves focus; only the focused one reads `textInput`/keys. Solves
  textInput for real (today any "focused" field reads everything).
- **Hit-test → per-node events:** the DOM/app already does hit-testing; formalize it in
  helpers: `onClick(rect)`, `onHover(rect)`, `onDrag(rect)` that return the state
  of that target (reading the raw input + comparing with the previous focused/hover).
- **Complete drag:** start (pressed over the target) → during (delta) → end
  (released) — kept in module-level state; a `beginDrag/dragDelta/
  endDrag` helper.
- **Double-click, key repeat:** already in the raw layer; the layer just exposes it ergonomically.

(Bubbling/propagation of DOM events is left for when the DOM facade has the
listener tree — a later phase; the browser needs it, immediate-mode UI does not.)

## Layer 3 — future (not now)
Touch/multitouch, gamepad, IME (composition for CJK), real clipboard (egui's
Copy/Cut/Paste already provide events), pointer lock.

## Implementation phases
1. **Complete keyboard + modifiers** (raw) — the biggest unlock. trait +
   egui maps `egui::Key`/`Modifiers`. ← start here.
2. **Rich mouse** (pressed/released/double/delta/drag/wheelX/setCursor) — raw.
3. **Focus + per-node events** (TS) — the usable layer.
4. **Drag helper + automatic cursor** (TS).

## Relation to the engine limits
Layer 2 (TS) runs into the same limits as `engine-limits-found-building-ui.md`
(state in module-level variables, no closures; number instead of a method bool).
The spec already assumes this — no callbacks, all polling + explicit state.
