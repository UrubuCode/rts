# rts-physics — the parallel rigid-body solver

**Read this file in full before changing anything in this crate.** The rules here
are binding for changes inside it. If a change requires breaking one, change the
rule first, with the reason.

---

## What this crate is

`rts:rigid`: a rigid-body solver in the gather/Jacobi formulation, spread over
`rayon`, reached from TypeScript as four `Float32Array`s in and the same four
written in place.

It is the **fallback**. The default backend for a scene of this shape is a GPU
compute kernel — `engine/rigid/gpurigid.ts` in the `rts-game` project — which does
the same work 22× faster at 2000 bodies. This crate exists for the machine with
no usable GPU.

## What it is not

A physics engine. There is no torque, no angular velocity, no joint, no convex
hull, no continuous collision. Two shapes exist: a sphere and an axis-aligned box.
`docs/colisores.md` in the game project says which of those absences are decisions
and which are waiting on something.

---

## The rules

### 1. This is a PORT, and the original is named in every file that ports it

The formulation, the constants, the order of the cases and the buffer layout come
from `engine/rigid/gpurigid.ts`. The two backends are compared against each other
by final position, so a rule silently improved here is a parity failure there —
and a parity failure reads as "the physics is strange on one of them", which is
the most expensive kind of bug this arrangement can produce.

**A change to a rule is a change to both, or it is a divergence.** A divergence is
allowed when it is stated: `solver/mod.rs` has one, in its module doc, with what
it costs.

### 2. No thread this crate starts may touch the engine

Not a `Context`, not a cell, not user code, not an allocation in the engine's
heap. A `Context` is reached through a thread-local, so a worker touching one is
looking at a runtime that is not its own.

The workers see `&[f32]` and `&mut [f32]`. That is why the surface is buffers
rather than objects with methods, and it is the same discipline `rts-node`'s ten
`thread::spawn` sites keep.

### 3. The borrow is dropped before the solver runs

`with_current` holds a `RefCell` borrow for the length of its body, and a panic in
an `extern "C"` frame cannot unwind — it aborts the process. Pointers are taken
inside a short borrow; every slice is built and used outside it.

### 4. Nothing is remembered between calls

No handle, no table of scenes, no view kept as a value. The collector cannot see a
value held outside the heap unless told to, so a remembered typed array is one
whose bytes are swept while this crate still points at them. Everything the step
needs is in its four arguments, sub-step count included (`world[3]`).

The one exception is scratch — a grid and two snapshot vectors — which holds no
body state and is overwritten at the top of every sub-step.

### 5. A refusal answers zero and simulates nothing

An argument that is not a typed array, two arguments over the same bytes, a length
that is not a whole number of records: the call answers `0` and no buffer is
touched. Half-simulating produces a scene that runs and is wrong, which is worse
than a call that plainly did not happen.

### 6. Files stop at 500 lines

Same ceiling as the rest of the workspace outside the two engine crates.

### 7. A number says what produced it, when, and on how many threads

There is a bench in `examples/onde_custa.rs` and it reproduces the scene the
TypeScript number was taken from, body for body, because a bench that builds its
own scene is not comparable to the one it is quoted against. Release only, and the
thread count is printed.

---

## Layout

```
src/
  lib.rs          the crate doc, and `install`
  surface.rs      `rts:rigid` — the two members, and the argument checks
  solver/
    mod.rs        the sub-step: integrate, statics, pairs, clamp, park, sleep
    contact.rs    the narrow phase: sphere, box, and the mixed case
    grid.rs       the broad phase: a spatial hash rebuilt per sub-step
```

## The buffer layout, which is the GPU backend's unchanged

| buffer | x, y, z | w |
|---|---|---|
| `pos` | centre | sleep counter (>= 10 is asleep) |
| `vel` | velocity | shape: 0 sphere, 1 box |
| `ext` | half-extent | inverse mass (0 is immovable) |
| `world` | `[0]` dt, static count, cell size, sub-steps; then static (centre, half-extent) pairs |

Unchanged so that one program can hand the same three arrays to either backend. A
conversion step between them would be one more place for the two to disagree.

`world[3]` is the one field this adds, and it was the layout's own unused slot: a
buffer written for the GPU backend still advances here, one sub-step at a time.

## What is NOT declared

`rts emit-types` answers from `#[rtse::class]`, which writes `crate::entry::…`
paths and therefore does not reach a host crate — the same limit `rts-std`'s
`abort.rs` states for property setters. So `rts:rigid` has no generated TypeScript
declaration, and a program importing it is typed by whatever it writes itself.
That is a gap and not a decision; it closes when the attribute learns to emit
outside `rts-core`, which `docs/engine/authoring-natives.md` §8 already lists as
open.
