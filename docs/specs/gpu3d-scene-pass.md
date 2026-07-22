# gpu3d — 3D scene pass under the egui overlay

**Status:** first slice (MVP) — colored-triangle meshes, depth-tested, camera,
per-draw transform. **Owner decision 2026-07-21.**

This is the first slice of the "scene rendering" phase that
`docs/specs/egui-ui-crate-design.md` §1b already reserved (P7+: *"expose
`wgpu::Device`/`Queue`/`Surface` + `beginScenePass`, with egui composited on
top"*). It is **out of scope of the HTML engine** (roadmap F0–F5 /
north-star) — the DOM pipeline is untouched; this is a sibling paint path that
renders BEFORE the egui pass in the same encoder/frame.

## What it is

A real 3D polygon pipeline (vertex/index buffers, WGSL shader, depth buffer,
perspective camera) rendered into the window's surface **before** the egui
pass, in the **same** `wgpu::CommandEncoder`. The egui UI (widgets, DOM/HTML)
composites on top as an overlay — the Unity-style "game view + UI" split.

```
frame:
  ┌─ scene pass (gpu3d) ── clear color+depth, draw meshes with MVP ─┐
  └─ egui pass ─────────── LoadOp::Load, UI on top ─────────────────┘
  one encoder, one submit, one present (vsync unchanged)
```

- No `egui::PaintCallback` — the scene is not an egui widget; it is the
  background of the whole window. This avoids the callback lifetime dance and
  keeps the egui pass byte-identical when no scene is drawn.
- wgpu backend only. On the glow backend the gpu3d calls are accepted but
  ignored (no-op) — same policy as `snapshot` (wgpu-only features warn once).
- When no `gpu3d.draw` happened in the frame, the egui pass keeps its current
  `LoadOp::Clear` behavior — zero cost, zero behavior change for existing apps.

## Doctrine compliance (PRIMORDIAL-vs-REGISTRY)

- Namespace `gpu3d` registered through the same Registry `Engine::ns` builder
  as `egui` (in `rts_egui::register`) — the engine never names it; dispatch is
  data-driven.
- Rust exposes raw primitives only (mesh upload from a `buffer` handle, camera
  set, draw). Ergonomics (Mesh/Camera classes, scene graph) belong to a TS
  layer later.
- Vertex/index data crosses the ABI as an existing `buffer` handle (u64) —
  the "opaque numeric slot" doctrine; no new value kinds.

## ABI surface (all functions take the window handle first)

| Member | Signature | Notes |
|---|---|---|
| `mesh` | `(win: u64, verts: u64, vertCount: i64) -> i64` | `verts` = buffer handle with `vertCount×6` f64 (x,y,z,r,g,b interleaved). Converted to f32 on upload. Returns meshId (>0) or 0 on error. |
| `meshIndexed` | `(win: u64, verts: u64, vertCount: i64, idx: u64, idxCount: i64) -> i64` | `idx` = buffer handle with `idxCount` i32 indices. |
| `meshFree` | `(win: u64, meshId: i64) -> void` | Frees GPU buffers of the mesh. |
| `camera` | `(win: u64, ex,ey,ez, tx,ty,tz: f64) -> void` | Look-at camera, up = +Y. |
| `perspective` | `(win: u64, fovYDeg, near, far: f64) -> void` | Aspect derived from the surface size each frame. Defaults 60°, 0.1, 1000. |
| `draw` | `(win: u64, meshId: i64, x,y,z, yawDeg, pitchDeg, scale: f64) -> void` | Queues one instance this frame. Model = T·Ry(yaw)·Rx(pitch)·S. List cleared after present. |
| `clearColor` | `(win: u64, r,g,b: f64) -> void` | Scene background (0..1). Default dark. |

Immediate-mode contract, matching the TS-driven loop: `draw` calls between
`beginFrame`/`endFrame` queue instances; `endFrame` renders and clears the
queue. Meshes/camera persist across frames.

## Implementation map

```
crates/rts-egui/src/scene3d/
  mod.rs       — SceneState (meshes, camera, draw queue), ABI fns, ns registration
  math3d.rs    — hand-rolled Mat4 (perspective, look_at, srt compose) — no new deps
  pipeline.rs  — WGSL shader, RenderPipeline, depth texture, the scene render pass
```

- `SceneState` lives in `RenderState` (per window, wgpu-only) as
  `Option<SceneState>`, created lazily on first `mesh` upload.
- Per-draw transform via **dynamic uniform offsets**: one uniform buffer with
  256-byte-aligned slots, one `mat4` (view_proj·model, composed on CPU) per
  draw, `set_bind_group` with dynamic offset. AOT-safe, no push constants.
- Depth: `Depth32Float` texture, recreated when the surface size changes.
- Draw-queue cap 4096/frame (silent drop beyond, logged once) — matches the
  uniform buffer size; raise when instancing lands.

## Deferred (later slices)

Textures/UVs + lighting (normals), instancing, mesh update-in-place (voxel
chunk streaming), render-to-texture (scene as DOM `<img>`/egui image),
`rts-gfx` crate rename (the design doc already authorizes it when scope grows).
