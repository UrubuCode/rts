# Design: immediate-mode GUI via egui — crate `rts-egui` + `ui` namespace

> **Status:** spec (pre-implementation). Architectural decision locked with the team;
> code comes after this spec. Branch: `feat/egui-ui-crate`.
>
> **This version (v2) was rewritten after exhaustive, adversarially verified
> technical research** into the real egui/winit/wgpu APIs (June 2026).
> Several v1 claims were corrected — see §13 (changelog) for the honest
> record of what was overly optimistic.

## 0. Executive summary

RTS gains a **cross-platform** native GUI based on **egui** (immediate-mode,
pure Rust, no C++ deps). The `rts:ui` documented as "FLTK 1.x" **was never
implemented** (zero code; only preparatory refs in the linker and one doc line) —
so this is a *clean slate*, not a migration: there is no FLTK to remove.

The project doctrine mandates: **the engine/Rust exposes only primitives; the
high-level API lives in TS.** This spec applies that to the letter:

- **Rust (`rts-egui`)** exposes egui's **immediate primitives** via a `u64`
  handle ABI (`extern "C"`): open window, pump events, begin/end
  frame, emit a widget and return its response (clicked? value?), present the
  frame. **Rust does not know the concept of a retained "Button" object** — it
  draws a button and returns "it was clicked".
- **TS drives the render loop** and builds the high-level library
  (`Window`, `Button` components, layout, state) on top of those primitives.

### What the research confirmed and what it corrected (TL;DR)

- ✅ **"TS drives the loop" is VIABLE** — winit exposes `pump_app_events` (non-blocking,
  returns control to the caller on each iteration). It is the official use case.
  Confirmed on **Windows/Linux**.
- ⚠️ **"100%-in-TS loop" was poorly grounded** — not because winit doesn't yield
  control (it does), but because the *body* of each step is non-primitive Rust
  state that doesn't cross the ABI. The correct model is **"TS-driven loop over
  Rust primitives"** (§1c).
- ⚠️ **"One widget = one FFI call" only holds for leaf widgets** — layout
  containers (`horizontal`/`Window`/`Grid`/`Panel`) are **closure-only** in egui's
  core (issue emilk/egui#1004). They require a **Rust-managed `Ui` stack** via
  `begin_container`/`end_container`. **This is the point of greatest uncertainty — it
  requires a dedicated PoC** (§2.2, §6).
- ⚠️ **macOS is a SOFT blocker, not fatal** — rendering outside the winit loop
  produces resize artifacts ("MacOS expects applications to render synchronously
  during `drawRect`"). That's why there is a **Model B (callback)** fallback (§5).
- ✅ **GC is not a risk for this feature** — it is inactive in the new engine; the other dev
  will refactor the GC later. The practical risk is per-frame handle **leakage**,
  which is mitigable (§4.3).
- ✅ **wgpu scene+overlay confirmed** — sustains the games/browser vision (§1b, §8).

### Locked decisions

| Topic | Decision |
|---|---|
| GUI lib | **egui** (immediate-mode, pure Rust, cross-platform) |
| FLTK | **Dropped** (never existed; update docs) |
| Crate | **New crate `rts-egui`** (do not stuff it into `rts-std`) |
| Who drives the loop | **Model A (TS drives): `while(ui.isOpen()){ pump → beginFrame → widgets → endFrame }`** primary on **Windows/Linux**. **Model B (callback): `ui.runApp(cb)`** fallback for **macOS/wasm** |
| Render backend | **wgpu primary/default** (modern GPU — games/browser); **glow** compat fallback |
| Window state (`UiCtx`) | **`thread_local! HashMap<u64, UiCtx>`** on the TS thread — it does **not** fit in the HandleTable `Entry` nor in `tokio_ctx` (winit/wgpu are `!Send`) |
| API depth | **Broad** (1:1 leaf widgets + containers via `Ui` stack), built in TS |
| Long-term vision | The crate is a **render foundation**: enables **games and even a browser engine** (custom wgpu scene + egui overlay) |
| Delivery | **Full spec first** (this doc); P1 is the **risk gate** before the broad API |

## 1. Why egui and why a new crate

- **Real cross-platform:** native Windows, macOS, Linux (Web/Wasm requires Model
  B — §5).
- **Pure Rust:** no C++/bindgen toolchain as FLTK would require; `runtime_support.a`
  doesn't need to embed C++ objects.
- **Immediate-mode matches the doctrine:** domain state lives in the *app* (in
  our case, in TS), and widgets are re-emitted each frame. That is
  exactly "primitives in Rust, logic in TS".
- **Isolated crate (`rts-egui`):** egui+winit+wgpu/glow bring a heavy dep graph
  (gfx, windowing). Isolating it in its own crate:
  - keeps `rts-std`/`rts-runtime` lean for those not using GUI;
  - allows **feature-gating** the backend (wgpu vs glow) without polluting the rest;
  - respects the per-crate complexity ceiling.

## 1b. Long-term vision: render foundation (games, browser)

This crate is **not "just widgets"** — it is **RTS's native render foundation**. The
team's stated goal is, in the future, to build **games and even a browser
engine** on top of it. That imposes requirements beyond a UI toolkit and
**shapes this spec's choices from the start** (even though the first delivery is only
GUI):

1. **Modern GPU is mandatory.** Games and a browser's layout/compositing need
   Vulkan/Metal/DX12, custom shaders, render targets, and drawing outside the UI.
   That's why **wgpu is the primary backend** (glow/OpenGL stays as the
   compatibility fallback). egui here is **an overlay** drawn on top of the scene.

2. **Access to the surface/device, not just to egui.** The research **confirmed** this
   is solid: `egui_wgpu::Renderer` was designed for the "you own the
   frame" model — `update_buffers`/`update_texture` outside the pass, `render(&mut
   RenderPass, jobs, screen_desc)` **inside a pass you open on your own
   encoder**. So you can: custom scene (`LoadOp::Clear` + geometry draw) →
   egui on the same encoder (second pass `LoadOp::Load`, doesn't erase the scene) → a
   single `queue.submit` + `frame.present()`. Official example: `custom3d_wgpu.rs`
   in `egui_demo_app`; `bevy_egui` confirms the pattern in production.

   ```
   beginFrame(h)          // pump input + ctx.begin_pass
     // [futuro] ui.beginScenePass(h): seu render de jogo/browser aqui (Clear+draw)
     ...widgets de UI...  // egui como overlay (LoadOp::Load no mesmo encoder)
   endFrame(h)            // end_pass + tessellate + egui render + submit + present
   ```

   **Honest caveat (verified):** the wgpu lifetime stitching becomes a *runtime*
   check: `egui_wgpu::Renderer::render` requires `RenderPass<'static>`, but
   `begin_render_pass` returns `RenderPass<'encoder>`; the bridge is
   `RenderPass::forget_lifetime()`, which **converts the "encoder untouched
   during the pass" validation from a compile error into a runtime error**. It works, but
   requires discipline — misuse becomes a runtime panic, not a build error.

3. **Full control of the loop is a requirement, not a preference.** A game loop needs
   `input → update → render scene → render UI → present` under the control of the user's
   code. Hence the "TS drives the loop" decision is not just ergonomics: it is a
   precondition for games. eframe (loop owner) is incompatible with that — which is
   why we use egui + winit + wgpu **without eframe**.

> **Scope of this delivery vs. the future.** The 1st delivery exposes an **immediate GUI** (window
> + widgets + loop). **Custom scene rendering** (geometry, shaders,
> textures) is a later phase (P7+), but the architecture here is designed to **not
> need to be redone** when that phase arrives. The crate may be renamed
> (`rts-gfx`/`rts-render`) when the scope grows beyond egui.
>
> **Browser via wasm is OUTSIDE Model A.** winit `pump_app_events` is
> incompatible with the Web (the browser doesn't yield a long-running external
> loop; events come via callback). If "browser vision" = *run in the browser via wasm*,
> it requires Model B (§5). If "browser vision" = *build a native browser
> engine with browser-style UI*, Model A sustains it.

## 1c. The loop model: what is TS, what is Rust (verified verdict)

The research demolished v1's naive grounding ("a 100%-in-TS loop is impossible
because winit doesn't return control"). **winit does return control** via
`pump_app_events` (takes `&mut self`, returns a `PumpStatus` on each call — the
opposite of `run_app`, which consumes the `EventLoop` and never returns). The real
impossibility is different, and simpler:

**What CAN stay in TS:**
- The iteration structure — the `while`, the exit test, the sequence of calls.
  It runs at top-level, on the main thread (the JIT calls `__rts_startup` synchronously, directly, without
  spawn — `crates/rts-codegen-new/src/front/run/module_jit.rs`).
- The stop condition (a `bool`/`i64` returned by a primitive).
- The application logic over the widgets' returns (app state in TS variables).

**What MUST stay in Rust:**
- `EventLoop`, `Window`, `wgpu::Device/Surface/Queue`, `egui::Context`, the root `Ui`
  — rich Rust types, mostly `!Send`/`!Sync`. **They don't fit in the HandleTable's `enum Entry`**
  (closed, primordial, in `rts-engine`) **nor in `tokio_ctx`**
  (which requires `Send+Sync`). Hence the `UiCtx` lives in a `thread_local! HashMap<u64, UiCtx>`
  and TS only holds an **opaque u64 handle**.
- The `pump_app_events(&mut event_loop, timeout, &mut app)` call requires live
  Rust references TS can't materialize. TS calls the primitive
  `__RTS_FN_NS_UI_PUMP`, which internally resolves the handle and makes the call.

**Conclusion:** there is no "loop without Rust in the frame path". There is a **"TS-driven
loop over Rust primitives"** — which is the correct and sufficient model.

## 2. The two hard points (and their solutions)

### 2.1 macOS: SOFT blocker, not fatal

Rendering driven from outside the winit loop (= per-frame `present()` called by
TS) is **discouraged by the winit doc on macOS**, verbatim: *"If you render
outside of Winit you are likely to see window resizing artifacts since MacOS
expects applications to render synchronously during any `drawRect` callback."*
Additionally, `pump_app_events` **stops the `NSApplication` between frames** (breaks
`rfd`/file dialogs) and the doc says *"You almost certainly shouldn't use this API."*

- **It is not a panic** on the happy path (main = process thread #0, no `block_on`
  on main). It is **visual degradation** (resize artifacts) — and the severity for the
  egui+wgpu/Metal stack **is not quantified in any source** (an empirical
  gap to measure in a macOS PoC).
- **Windows/Linux are first-class** in Model A (Windows uses `PeekMessage`,
  genuinely non-blocking).
- **Solution:** Model B (callback, §5) where winit owns the loop and the draw sits
  inside the handler — portable on macOS, and the only path on wasm.

### 2.2 Layout containers: closure-only in egui (the biggest risk)

The research **refuted** "one widget = one FFI call" in the general case:

- ✅ **Leaf widgets** (`button`, `slider`, `label`, `text_edit`) return an
  **owned, lifetime-free** `Response` → `ui.button(x).clicked()` produces a `bool`
  that crosses `extern "C"` directly. Confirmed officially + by production bindings
  (**Egui.NET** C# ~97% of the API one-call-per-method; **pyegui** Python
  one-widget-per-call).
- ⚠️ **Containers** (`Ui::horizontal`, `egui::Window`, `CentralPanel`, `Grid`)
  take `FnOnce(&mut Ui)` — **there is no begin/end API in egui's core** (issue
  emilk/egui#1004, acknowledged without a solution). A button inside a `Window` requires
  a child `Ui` born inside the container's closure.

**Solution (viable, more laborious, NOT proven running — only inferred from the
signatures):** model `begin_container`/`end_container` by stacking `Ui`s
manually on the Rust side. The root `Ui` is created via `Ui::new(ctx, id, UiBuilder)`;
each container allocates a child `Ui` (via `UiBuilder`/`allocate_ui*`) and pushes it
onto a **`Ui` stack in the `thread_local`** indexed by the frame handle.
`begin_container` pushes; subsequent widgets operate on the top; `end_container`
pops and finalizes — reimplementing what `.show()` encapsulates.

> **This is the point P1 MUST prove before writing the broad API.** No
> source shows this manual `Ui` stack running; it is the spec's biggest uncertainty.

## 3. Crate architecture

```
crates/
  rts-egui/                      ← NOVA crate
    Cargo.toml                   features: ["wgpu"] (default), ["glow"]
    src/
      lib.rs                     register(e: &mut Engine) + re-exports
      abi.rs                     NamespaceMember[] do namespace `ui` (símbolos)
      ctx.rs                     UiCtx + thread_local HashMap<u64, UiCtx>:
                                 EventLoop, Window, egui::Context,
                                 egui_winit::State, renderer wgpu/glow,
                                 pilha de Ui do frame corrente
      app.rs                     openWindow/close/isOpen/pump (Modelo A) +
                                 runApp callback (Modelo B) (extern "C")
      frame.rs                   beginFrame/endFrame (extern "C")
      widgets/                   um arquivo por família de widget:
        button.rs                button, checkbox, radio
        text.rs                  label, heading, text_edit
        value.rs                 slider, drag_value, progress_bar
        layout.rs                begin/end de horizontal/vertical/grid/panels
        window_widgets.rs        begin/end de egui::Window
      handle.rs                  encode/decode handle u64 <-> slot UiCtx
```

> No file above may exceed 500 lines (project rule) — `widgets/`
> is already a folder of cohesive submodules for that reason.

- `rts-egui` depends on `rts-engine` (ABI/Engine, registration) — **not** on
  `rts-codegen-new` (doctrine: engine doesn't name non-primordials; `ui` resolves via
  the Registry).
- `rts-runtime` re-exports `rts-egui` (as it already does with `napi`) behind a
  `ui` feature (default on for desktop; off on wasm).
- The linker (`rts-linker`) removes the FLTK refs (X11/Pango/etc. libs) and links the
  window/GPU deps per backend: **wgpu** → Vulkan/Metal/DX12 loader +
  windowing (xkbcommon, x11/wayland on Linux); **glow** → libGL/EGL. Document the
  per-platform matrix.

## 4. Loop and threading model (detailed)

### 4.1 Per-window state

- **One window = one `UiCtx`** in a **`thread_local! HashMap<u64, UiCtx>`** (not in
  the HandleTable — `UiCtx` is `!Send`). Handle `u64` = opaque key, in the style of the
  other namespaces, but the storage is local to the TS thread.
- The `UiCtx` holds: `EventLoop`, `Window`, `egui::Context` (refcounted, cheap
  clone), `egui_winit::State`, the renderer (`egui_wgpu::Renderer` or
  `egui_glow::Painter`), and the current frame's **`Ui` stack**.

### 4.2 Cycle (Model A — TS drives)

- `openWindow(title, w, h, backend)`: creates the `EventLoop` (on the calling thread;
  **main-thread #0 on macOS**) + `Window`; creates `egui::Context` +
  `egui_winit::State`; initializes the renderer (wgpu init is async → resolved with
  `pollster::block_on` in a synchronous call); stores it in the `UiCtx`; returns the handle.
- `pump(h) -> i64`: `event_loop.pump_app_events(Some(Duration::ZERO), &mut app)`;
  the internal handler accumulates `WindowEvent`s via `egui_winit::State::on_window_event`
  and handles `CloseRequested`. Returns `0` = continue, `!=0` = exit.
- `beginFrame(h)`: `let input = state.take_egui_input(window); ctx.begin_pass(input);`
  creates the root `Ui` and pushes it onto the `UiCtx`'s `Ui` stack.
- `widget(h, …)`: operates on the `Ui` at the top of the stack; returns the primitive
  **response** (`clicked: bool`; `f64`; string-handle). **The primitive is extracted in the same
  call — the `Response` is never retained across the FFI.**
- `beginContainer/endContainer(h, …)`: pushes/pops a child `Ui` (§2.2).
- `endFrame(h)`: `let out = ctx.end_pass();` → `tessellate` →
  `state.handle_platform_output` → `renderer.update_texture/update_buffers` →
  `surface.get_current_texture` → encoder → [optional custom scene] → egui pass
  (`LoadOp::Load` + `forget_lifetime`) → `queue.submit` → `frame.present()` (vsync)
  → `free_texture`.
- `isOpen(h)`: false after `CloseRequested`. `close(h)`: drops the `UiCtx`, frees the slot.

### 4.3 Threading and GC

- Loop + widgets run **on the same thread** as the TS program (the main #0). No
  cross-thread callback calls in Model A — TS calls Rust, not the reverse.
- **GC: not a concern for this feature.** Verified in code: the GC is
  **inactive** in the new engine (`install_gc_hook` is never called; `is_active()`
  always false; the main thread doesn't register in the `thread_registry`, so
  `SuspendThread` never touches it). The other dev will refactor the GC later.
- **The real risk is leakage, not deadlock:** without GC, strings/arrays allocated per
  frame **leak** for the process's lifetime. **Mitigation:** widgets should
  accept strings via `ptr+len` of a **reused buffer**, not allocate a new
  string handle per label per frame; reuse buffers in the app; `string_free`/
  `RTS_AUTO_FREE_HANDLES=1` for the unavoidable. P1 measures memory growth
  under N thousand frames.

## 5. Model B (callback) — macOS/wasm fallback

Where Model A degrades (macOS: resize artifacts) or is impossible (wasm: no
external loop), control inverts: **winit owns the loop** (`run_app`/
`ApplicationHandler`) and calls TS back per frame.

```ts
// TS — the loop belongs to Rust; the frame is a TS callback:
ui.runApp({ title: "Demo", width: 800, height: 600 }, (app) => {
  ui.label(app, "Olá");
  if (ui.button(app, "Salvar")) salvar();
});
// runApp only returns when the window closes
```

Internally: on `RedrawRequested` Rust invokes the TS `fn_ptr` via a **direct** C-ABI call,
**on the same thread, without spawn and without `CallConv::Tail`** — the safest case of a
Rust→TS call. The internal memories of bugs #206 (Tail callconv × extern "C" on a
new thread) and #1556 (parallel async race + GC) **do not apply**: they belong to the old
engine / the parallel async path, not to this synchronous same-thread
invocation. The draw happens inside the handler → **portable on macOS** (synchronous render in
`drawRect`), and it is the only model on wasm.

**The widget facade (`ui.label/button/slider`) is identical in both models** —
only who drives the loop changes. The dev (or the TS lib) chooses by platform/intent.

## 6. ABI surface (`extern "C"` primitives)

Convention `__RTS_FN_NS_UI_<NAME>`. Strings come in as `StrPtr` (ptr+len);
**prefer a reused buffer** over a new string-handle per frame (§4.3). Widgets return
primitives (bool→i64 0/1, f64, u64). **No polymorphic value crosses the boundary.**

### 6.1 Lifecycle / loop

| Símbolo | Assinatura (lógica) | Retorno | Modelo |
|---|---|---|---|
| `ui.openWindow` | `(title, w:i32, h:i32, backend:i32)` | `handle u64` | A |
| `ui.pump` | `(h)` | `i64` (0=continue) | A |
| `ui.isOpen` | `(h)` | `bool` | A |
| `ui.beginFrame` | `(h)` | `void` | A |
| `ui.endFrame` | `(h)` | `void` | A |
| `ui.close` | `(h)` | `void` | A |
| `ui.runApp` | `(title, w, h, backend, frame_fn: handle)` | `void` | B |
| `ui.setTitle` | `(h, title)` | `void` | — |

`backend`: `0 = wgpu` (default), `1 = glow`.

### 6.2 Leaf widgets (solid — 1:1 via FFI)

| Símbolo | Assinatura | Retorno |
|---|---|---|
| `ui.label` | `(h, text)` | `void` |
| `ui.heading` | `(h, text)` | `void` |
| `ui.button` | `(h, label)` | `bool` (clicado) |
| `ui.checkbox` | `(h, label, checked: bool)` | `bool` (novo estado) |
| `ui.radio` | `(h, label, selected: bool)` | `bool` |
| `ui.slider` | `(h, value: f64, min: f64, max: f64)` | `f64` (novo valor) |
| `ui.dragValue` | `(h, value: f64)` | `f64` |
| `ui.progressBar` | `(h, fraction: f64)` | `void` |
| `ui.textEdit` | `(h, text_handle: u64)` | `u64` (novo string-handle) |
| `ui.separator` | `(h)` | `void` |

### 6.3 Containers (via the `Ui` stack in Rust — requires PoC, §2.2)

Begin/end pairs that push/pop a child `Ui` on the `UiCtx`'s stack:

| Símbolo | Efeito |
|---|---|
| `ui.horizontalBegin` / `ui.horizontalEnd` | layout horizontal |
| `ui.verticalBegin` / `ui.verticalEnd` | layout vertical |
| `ui.gridBegin(cols)` / `ui.gridEnd` | grade |
| `ui.windowBegin(title)` / `ui.windowEnd` | `egui::Window` flutuante |
| `ui.centralPanelBegin` / `ui.centralPanelEnd` | painel central |
| `ui.sidePanelBegin(side)` / `ui.sidePanelEnd` | painel lateral |

> The begin/end pairs **do not exist natively in egui** (they are closure-only); this
> layer synthesizes them by managing the `Ui` stack manually. That's what the high-level
> TS API hides: the `Window` component does `windowBegin()` at the start of the
> scope and `windowEnd()` at the end.

## 7. High-level TS layer (where the ergonomics live)

Per the doctrine, the component library **is not Rust** — it is TS over the
primitives. Location: builtin package `builtin/ui/` (the pattern of the other builtins:
`console/`, `globals/`). Final-usage sketch (Model A):

```ts
import { App, Label, Slider, Button } from "rts:ui";

const app = new App("Demo", 800, 600);   // ui.openWindow por baixo
let volume = 0.5;

app.run(() => {                          // app.run roda o while + pump por baixo
  Label("Volume");
  volume = Slider(volume, 0, 1);
  if (Button("Mute")) volume = 0;
});
```

`App.run(frameFn)` in TS is literally the TS-driven loop:

```ts
run(frameFn) {
  while (ui.isOpen(this.h)) {
    if (ui.pump(this.h) !== 0) break;   // bombeia eventos do SO
    ui.beginFrame(this.h);
    frameFn();                          // o dev emite widgets aqui
    ui.endFrame(this.h);                // tessela + render + present
  }
  ui.close(this.h);
}
```

On macOS/wasm the same `App` uses `ui.runApp(cb)` (Model B) underneath — the
public API for the end dev is the same.

## 8. Selectable backend (wgpu primary, glow fallback)

- `rts-egui` declares `wgpu` (**default**) and `glow` features, both compilable
  together; `openWindow`'s `backend: i32` chooses at runtime when both are
  present.
- **wgpu (primary/default)**: native Vulkan/Metal/DX12/GL, WebGPU/WebGL2 on wasm.
  It is the path for the long-term vision (§1b): custom scene + egui overlay. Async
  init (`request_adapter`/`request_device`) resolved with `pollster::block_on`.
  Compiles heavier — the cost of a modern GPU.
- **glow (compat fallback)**: OpenGL/ES, light deps, good in VMs/old machines
  or where wgpu doesn't initialize. Supports the immediate GUI, **but not** the
  advanced scene rendering of the future phases.
- **Future hook (P7+):** the `UiCtx` holds `wgpu::Device`/`Queue`/`Surface` in a
  way that a later phase can expose `ui.beginScenePass(h)` for custom
  rendering, with egui composited on top in `endFrame` (§1b). The `UiCtx` is
  structured to support it without refactoring.

## 9. Versioning (coherent set, June 2026)

egui breaks API between minors; egui/egui-wgpu/egui-winit must stay **in lockstep
(same number)**, and wgpu/winit are pinned by them. **Pick egui first,
match the rest.**

```toml
egui       = "0.34.3"   # escolher primeiro
egui-wgpu  = "0.34.3"   # MESMO número do egui (senão diverge tipos de epaint)
egui-winit = "0.34.3"   # MESMO número do egui
wgpu       = "29.0"     # fixado por egui-wgpu 0.34
winit      = { version = "0.30", features = ["pump_events"] }  # pump_app_events
egui-glow  = "0.34.3"   # backend fallback
pollster   = "0.4"      # block_on do init async numa openWindow síncrona
```

Notes that go into the `Cargo.toml`/code:
- **winit 0.30.x:** the `pump_events` feature must be enabled explicitly;
  `pump_app_events` is the method (the old `pump_events` closure is deprecated). In the
  `ApplicationHandler` model (winit 0.30) the internal handler accumulates the events.
- **egui 0.34:** `begin_pass`/`end_pass` remain; `Context::run` became
  `run_ui`, and the `on_begin_pass`/`on_end_pass` hooks became a `Plugin` trait in 0.33 —
  irrelevant for the manual begin/end, but re-validate names when bumping the version. (If
  you prefer the stability of the non-deprecated `run`, 0.31.1/0.32.x are an option, with the
  corresponding wgpu/winit lockstep — check.)
- **wgpu 29:** `request_device` returns `(Device, Queue)` in a future;
  `RenderPassColorAttachment` has `depth_slice` (wgpu 26+); `render()` requires
  `RenderPass<'static>` via `forget_lifetime()`.
- **No eframe** (we need control of the loop; eframe would own it).

## 10. Implementation phases

> Each phase runs the suite incrementally. **P1 is the viability gate** — do not
> write the broad API before P1 passes.

- **P0 — Crate skeleton.** `rts-egui` compiles empty, registered behind the
  `ui` feature; `cargo build` green; `rts apis` lists the `ui` namespace (no
  members). Linker stops referencing FLTK.
- **P1 — Loop + containers PoC (RISK GATE).** Backend **wgpu**, Model A,
  Windows. Prove, in this order:
  1. `openWindow` on the main thread with tokio already initialized **without panic**;
  2. `pump(ZERO)` + `beginFrame`/`endFrame` + `present()` running in a TS `while`;
  3. **the manual `Ui` stack for containers** (§2.2): open a `Window`/
     `horizontal`, emit 3+ nested widgets, verify correct visual layout —
     **the most uncertain item, with no precedent in any source**;
  4. custom scene (`Clear`) + egui overlay (`LoadOp::Load` + `forget_lifetime`) on the
     same encoder, without a runtime error;
  5. N thousand frames measuring memory growth (leakage §4.3).
  If (3) fails, re-evaluate the container API (e.g. expose only pre-built layouts).
  If macOS resize is severe (measure if hardware is available), make Model B
  mandatory on macOS.
- **P2 — Leaf widgets.** checkbox, radio, slider, dragValue, textEdit, separator,
  progressBar, heading (all 1:1, the solid path).
- **P3 — Containers.** horizontal/vertical/grid + Window/CentralPanel/SidePanel
  (depends on P1.3 having validated the `Ui` stack).
- **P4 — Model B (callback) + glow backend.** `runApp(cb)` for macOS/wasm;
  `egui_glow` behind the feature; per-platform linker matrix.
- **P5 — High-level TS layer** (`builtin/ui/`): `App`, `Window`, `Button`,
  `Slider`, `Label`, etc., with `.d.ts`. The `App` picks Model A/B per platform.
  Examples + tests.
- **P6 — Docs.** Update `CLAUDE.md` and `01-architecture.md` (swap "FLTK 1.x"
  for "egui"), entry in `docs/specs/INDEX.md` (already done), remove FLTK refs from
  the linker.
- **P7+ (future, outside this delivery) — Scene rendering (games/browser).** Expose
  `wgpu::Device`/`Queue`/`Surface` + `beginScenePass`: clear the screen, custom
  render pass, geometry/shaders/textures, egui composited on top (§1b). Possible
  crate rename (`rts-gfx`/`rts-render`).
  **LANDED, then superseded (2026-07-23):** the first slice was a namespace
  `gpu3d` (mesh/camera/draw in a scene render pass before the egui pass, same
  encoder, no PaintCallback — egui as the overlay). It was reconciled with the
  richer scene pass from `feat/rts-egui-3d-scene-editor` (shadow map/PCF, point
  light, specular, skybox, procedural texture, emissive, instancing), which won
  as the base; the good parts of `gpu3d` were grafted into it. **The live API is
  the `egui.*` namespace** (`egui.meshUpload/drawMesh/setCamera/…`) — `gpu3d` no
  longer exists, and its spec file was deleted with it.

## 11. Risks and mitigations (updated by the verdict)

| Risk | Severity | Mitigation |
|---|---|---|
| **Closure-only containers** (manual `Ui` stack, no precedent in any source) | **High — biggest uncertainty** | **P1.3 is the gate**; if unviable, expose pre-built layouts instead of arbitrary begin/end |
| **macOS: resize artifacts** (render outside `drawRect`) | Medium (soft, not fatal) | Model B (callback) on macOS; measure real severity in a macOS PoC |
| **wasm incompatible with Model A** | Medium | Model B is the only path on wasm; document it |
| **`forget_lifetime` becomes a runtime error** | Medium | Discipline: don't touch the encoder during the pass; cover in a test |
| **egui/wgpu version lockstep** | Low | Pinned versions (§9); CI compiles the crate |
| **Per-frame handle leakage** (GC inactive) | Medium | Reused buffers (`ptr+len`), not a new string-handle per frame; P1.5 measures |
| **Compile weight (wgpu)** | Low | Isolated crate behind the `ui` feature; doesn't affect non-GUI builds |
| **Future GC reactivated + render thread registered** | Low (future) | Keep the render thread **out** of the `thread_registry`; cooperative safepoints — **the responsibility of the other dev's GC refactor** |

## 12. Doctrine impact (check)

- ✅ The engine doesn't name `ui` (resolves via Registry/`registry_build.rs` — today `ui`
  is on the "deliberately absent" list; it leaves it and enters the `REGISTER[]` behind the
  `ui` feature).
- ✅ Rust exposes **only primitives**; ergonomics in TS (`builtin/ui/`).
- ✅ No high-level builtins in the engine; no Symbol; no hardcoding of a
  non-primordial name in the front.
- ✅ Isolated crate respects the complexity ceiling and the layer partitioning;
  no file > 500 lines.
- ✅ JIT symbol derived from `SPECS` (`abi_gen.rs`), no manual `add_fn!`.

## 13. Changelog v1 → v2 (corrections after verified research)

Honest record of what v1 claimed too optimistically (see the adversarial verdict):

1. **"glow default" → "wgpu default".** The ecosystem uses wgpu as the primary
   renderer; glow is the alternative. (Also: wgpu dropped the `0.` and is at 29.0.x, not
   "0.20/0.22".)
2. **"a 100%-in-TS loop is impossible because winit doesn't yield control" → corrected.**
   winit **does yield** control (`pump_app_events`). The real impossibility is the
   non-primitive Rust state that doesn't cross the ABI. Correct model: "TS-driven loop
   over Rust primitives" (§1c).
3. **"one widget = one FFI call" → only leaf widgets.** Containers are
   closure-only (egui#1004); they require a manual `Ui` stack in Rust (§2.2) — the biggest
   risk, to be proven in P1.
4. **"works the same on every platform" → corrected.** macOS degrades (resize),
   wasm requires Model B. Windows/Linux first-class in Model A; Model B (callback)
   added as a fallback (§5).
5. **"GC managed/safe" → corrected.** The GC is inactive in the new engine; no
   deadlock, but there is per-frame handle **leakage** (§4.3). GC is the
   responsibility of the other dev's future refactor, not this feature's.
6. **`UiCtx` in the HandleTable → corrected.** It doesn't fit in the `Entry` (closed,
   primordial) nor in `tokio_ctx` (`Send+Sync`; winit/wgpu are `!Send`). It goes in a
   `thread_local! HashMap<u64, UiCtx>` (§4.1).
