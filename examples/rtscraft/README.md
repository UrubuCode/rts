# RTS-MINE

A first-person voxel "Minecraft" running 100% on the RTS runtime (TypeScript →
native via Cranelift JIT), structured **Unity-style**: GameObjects + Scene +
polymorphic `update(dt)`. Software renderer — every pixel raycasts a 3D DDA
through the voxel grid in pure TS; the RGBA framebuffer is presented through
`render.image` (egui backend just paints).

```
E:\rts\target\release\rts.exe run E:\RTS-MINE\main.ts
```

## Controls

| Input | Action |
|---|---|
| WASD | walk (yaw-relative) |
| Mouse | look (relative delta; no pointer-lock yet — keep cursor inside the window) |
| Arrows | look (fallback) |
| Space | jump / swim up |
| Left click | break targeted block |
| Right click | place selected block |
| Scroll or 1-5 | select hotbar block (grass/stone/sand/log/leaves) |
| Esc | quit |

## Architecture (Unity-like, Minecraft-style per-layer renderers)

```
engine/
  core.ts     — Transform, GameObject (start/update/destroy, spriteKind), Scene
  world.ts    — class World: voxel grid (get/set/solid/topY) + terrain generate()
  input.ts    — class Input: Unity-style axes (axisH/axisV/jump) + mouse deltas
  render3d.ts — ENTITY renderer: billboards projected into the framebuffer with
                DDA occlusion (mobs are NOT blocks in the grid)
game/
  player.ts   — class PlayerController extends GameObject (look + walk + physics)
  slime.ts    — class Slime extends GameObject (wandering, hopping; rendered as
                an animated billboard sprite with eyes)
main.ts       — bootstrap: new World/Scene, scene.add(player, slimes...), loop
                calls scene.update(dt), then world render → entity render →
                HUD/menu (immediate canvas, no DOM)
raycast.ts    — WORLD renderer: per-pixel 3D DDA over raw buffers (checkerboard)
                + castEditRay (crosshair target)
config.ts     — window/renderer/world dimensions, key codes, stbuf contract
bench.ts      — headless renderer benchmark (ms/frame, no window)
```

Each layer has its own renderer, Minecraft-style: world (raycaster), entities
(billboards), UI (immediate canvas). Resolution is configurable in-game
(Esc menu, 5 presets 192×128 → 384×256; framebuffer allocated for the max).
The `stbuf` (f64-per-slot buffer) is the **renderer state** (camera, crosshair
target, active resolution, clock); gameplay state lives in GameObject fields.

Note: a DOM-based menu (`rts:dom` + `egui.render` over the game) was validated
and worked — replaced by the canvas menu to keep the Minecraft-style "own
renderer" approach. DOM-over-game remains a proven option (see issue #1872).

Capability probe (2026-07-05): inheritance, virtual dispatch over
`GameObject[]`, instances as params/fields, nested field mutation
(`this.transform.x`), namespace calls inside methods, buffer handles in class
fields — ALL work on the current engine. The old OOP limits are gone.

Renderer budget (measured): 192×128 internal resolution ≈ 21 ms/frame headless
(~47 FPS ceiling); in-game ~60 FPS with vsync at 960×640 window scale.

## RTS engine limits this codebase works around (learned the hard way)

1. **`let` initialized with an int-valued literal is classified as integer**
   (issue #1869) — `let vy = 0.0; vy = vy - 0.27` stays `0`, silently. Class
   FIELDS are fine. **Fix for lets:** annotate `let vy: f64 = 0.0`.
2. **Handles passed as `number` parameters corrupt** (fcvt instead of bitcast;
   issue #1870) — `buffer.read_f64` in the callee returns NaN. Handles in class
   fields are fine. **Fix:** annotate handle params as `i64`.
3. **Top-level consts referenced as call arguments inside a function don't
   capture** (#1726) — key codes live as literals inside `engine/input.ts`
   methods for this reason.

(Corrected: an earlier note here claimed inline-compared call returns take the
wrong branch — that does NOT reproduce on the current engine; the symptom was
#1869. Extracting comparisons to consts is still fine style, not a requirement.)

Feature issues filed from this project: #1871 pointer-lock for mouse-look,
#1872 DOM mouse-event bridge (menu uses manual hit-testing today),
#1873 buffer load/store intrinsics (raycaster perf), #1874 eval-mode namespace
resolution inside class methods.

## Stability

Verified: a 22-minute continuous session (Scene + GameObjects allocating
normally) ended only by the user closing the window — no GC/handle issues.

## Next steps

- Billboard sprites for mobs (instead of block bodies).
- Pointer-lock / cursor confinement in `rts:input` for true mouse capture.
- Chunked world + larger terrain (current: 64×32×64 single chunk).
- Save/load world (fs namespace).
