# New-engine limits found while building the UI (map for the future)

> A side — and valuable — result of the UI stack experiment (DOM + canvas +
> components in TS over `rts:render`). Each limit below was found IN
> PRACTICE while writing real UI, with the workaround we used and what the engine
> would need to implement "in the future". It is a list prioritized by real usage, not
> theoretical. Dated 2026-06-25, engine `rts-codegen-new`.

## How to read

Each item: **what fails** · **where it hit** · **current workaround** · **what the
engine needs**. Ordered by how much it hurts UI ergonomics.

---

### 1. Top-level `const` doesn't capture inside a function
- **Fails:** `const SLOT = 3; function f() { usa SLOT }` → `unbound identifier`.
- **Where:** layout-TS (style slots), any constant used in a helper.
- **Workaround:** inline literals, or module-level variables (`let`), or rehydrate
  inside the function.
- **Engine needs:** capture top-level bindings (const/let) in the scope of nested
  functions. See `project_multi_declarator_capture_bug`.

### 2. Method on the RETURN of a getter/call doesn't dispatch
- **Fails:** `const cv = app.canvas; cv.box(...)` or `app.canvas.box(...)` →
  `receiver class not statically dispatchable` / unproven shape.
- **Where:** App exposing `canvas` via getter; any facade returning an object
  for chaining.
- **Workaround:** DIRECT methods on the already-proven instance (`app.box()` instead of
  `app.canvas.box()`); the facade delegates internally.
- **Engine needs:** propagate the return class of getters/methods to enable
  dispatch on the returned value (part of this already progresses for method-of-call via `local_classes`;
  getter and field are missing). See `project_new_engine_dispatch_limits`.

### 3. `boolean` returned from a method doesn't coerce in `?:`/condition
- **Fails:** `let on = obj.method(); const c = on ? a : b;` →
  `cannot coerce Tagged to Bool`.
- **Where:** checkbox/toggle returning `boolean`.
- **Workaround:** use `number` (0/1) instead of `boolean` for state coming from a
  method; compare with `!== 0`.
- **Engine needs:** Tagged→Bool coercion on the method-return path (today the
  method bool arrives Tagged and `?:` doesn't coerce).

### 4. `.length` on a string from a method/param/reassigned
- **Fails:** `function f(s: string) { s.length }` or `const t = obj.m(); t.length`
  → `.length on a receiver of unproven shape — dynamic-length is a separate path`.
- **Where:** textInput (measuring/trimming typed text).
- **Workaround:** avoid `.length` on those strings; only concatenate (`a + b`
  always works). Backspace/text editing stay blocked without this.
- **Engine needs:** resolve `.length` (and probably `.substring`/indexing) on a
  string of unproven shape — route to the dynamic string path.

### 5. Array-indexed string + subsequent use
- **Fails:** `const names = ["a","b"]; app.tab(..., names[i], ...)` bails in some
  uses (the string element has no proven shape).
- **Where:** list of tab/item names.
- **Workaround:** direct literals, or parallel arrays of primitives.
- **Engine needs:** proven shape for `array[i]` when the array is of string/obj.

### 6. `performance.now()` returns 0
- **Fails:** `performance.now()` always returns `0` (delta time impossible through it).
- **Where:** App loop / animation.
- **Workaround:** use `rts:time` (`time.now_ms()`, monotonic) — works.
- **Engine needs:** wire `performance.now()` to the real clock (today the
  `performance` prelude has no effective timeOrigin/now on the current path).

### 7. Input readable only INSIDE the frame
- **Not a bug — a usage rule, document it:** reading `input.*` BEFORE
  `beginFrame` returns empty state; `beginFrame` is what transfers OS events into
  the context. Always read input after `beginFrame`.

---

## What was NOT a limit (already works — good to know)

- **Classes with methods + chaining** (`new C().m().m()`): OK.
- **Property getter/setter** (`el.textContent = x`): OK (it is the basis of the real
  DOM facade).
- **Global singleton object via prelude** (`console`/`document`/`createApp` style):
  OK.
- **Method returning `T | null` + `=== null`/`if(x)`**: OK (as long as it is a class
  method, not a free function).
- **Array of instances** (length/indexing/for-of/push): OK.
- **String coming from Rust** (GC handle) used as a string: OK.
- **Multi-window**: OK (UiCtx per handle; global pump per WindowId).
- **Deep recursion + many ABI calls per frame** (layout): OK.

## Conclusion

Despite 6 real limits, it was possible to build: a real DOM (MDN), layout in TS,
abstract render/input (pluggable backend), an ergonomic canvas, an App loop with delta
time, and a component library (button/slider/checkbox/progressBar/panel/
tabs/textInput/automatic-layout) + multi-window. The foundation is solid; the 6 items
above are the engine's refinement roadmap for rich UI — implement in the future,
highest-impact-first (1, 2 and 4 unlock the most).

---

### 8. `moveWindow`/`set_outer_position` only applies AFTER the loop starts
- **Not a bug — winit timing:** calling `app.moveTo(x,y)` BEFORE the first
  `beginFrame`/`pump` doesn't reposition the window (the event loop hasn't run yet).
- **Workaround:** call `moveTo` INSIDE the loop, after a few frames
  (`if (frameCount() > 2)`), once.
- **Engine/backend could:** apply the pending position on the first pump, or expose
  an initial position in `openWindow`.

---

### 9. Returning a string from the ABI: use `AbiType::Handle` LITERALLY, not the `U64` alias
- **Fails:** a fn returning a string (GC handle) declared with `Sig::new(..,
  Handle)` where `Handle` is a local alias `U64 as Handle` → TS receives the raw handle
  NUMBER ("pointer data"), not the string. `typeof` = "number".
- **Where:** `input.textInput` (and any getter of a new string in a crate using
  the `U64 as Handle` alias for resource handles).
- **Cause:** the engine only reboxes the return as `TAG_STR` when it is `AbiType::Handle`
  LITERALLY **AND** the `ts_signature` ends in `: string`. The `U64` alias falls into the
  raw-integer branch.
- **Workaround/rule:** STRING returns use explicit `AbiType::Handle` (even
  when resource-handle ARGS use the `U64` alias). Already fixed in getText/
  getAttribute/tagName (rts-dom) and textInput (rts-render).

---

### 10. TRAP: namespace members are snake_case (NOT camelCase) — no normalization
- **Fails (looked like a serious bug):** `import audio from "rts:audio";
  audio.openOutput(...)` → "no matching namespace function (...)". Likewise
  `audio.default_sample_rate` seemed to fail. Initial conclusion WRONG: "audio isn't
  wired into the JIT".
- **REAL cause:** the registered member is `open_output` (snake_case, as in Rust),
  and the engine does **NOT normalize camelCase→snake** when resolving a namespace
  member (`registry::namespace_member` compares the literal name). `audio.open_output(...)`
  WORKS (tested: 440Hz beep plays, user confirmed).
- **Rule:** when importing from `rts:<ns>`, use the member name EXACTLY as
  registered (snake_case for backend namespaces: `open_output`,
  `default_sample_rate`, `write_f32`, etc.). The ergonomic preludes (App, console,
  document) are what expose camelCase; the raw namespace is snake.
- **`rts:audio` WORKS** (cpal): open_output/write/close/sample_rate/channels/
  master_volume/available_frames/queued_frames. Model: generate f32 samples in an
  `rts:buffer` → `audio.write(dev, buf, n)`. Music/effects validated in Pong.
