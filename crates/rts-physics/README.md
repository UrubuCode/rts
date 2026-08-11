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

### 4. No ENGINE VALUE is remembered between calls

No handle to a cell, no table of scenes, no view kept as a value. The collector
cannot see a value held outside the heap unless told to, so a remembered typed
array is one whose bytes are swept while this crate still points at them.
Everything the step needs about the *program* is in its four arguments, sub-step
count included (`world[3]`).

**This rule was "nothing is remembered between calls" and was narrowed on
2026-08-11, with the reason.** As written it forbade all state, and the reason it
gave — the collector cannot see a value held outside the heap — is a statement
about *engine values*, not about memory. The scratch exception already showed the
seam: a grid and two snapshot vectors persist and always did, because they hold no
body state.

What the wording cost is a whole class of backend. Every third-party engine worth
plugging in — Rapier, PhysX, Jolt — keeps a persistent world: bodies with stable
identity, and contact manifolds carried between frames. That persistence is not an
implementation detail to be optimised away; it is **where their solvers get both
their accuracy and their speed**, because warm-starting a contact from last
frame's impulse is what makes a stack settle instead of jitter. A rule forbidding
it forbids them, and it forbade them for a reason that does not apply to them: a
Rapier world holds Rust-owned bodies, not cells, so there is nothing in it the
collector could sweep.

So the line is drawn where the hazard actually is:

| may persist | may NOT persist |
|---|---|
| scratch (grid, snapshots) | a cell, a `Value`, a slot |
| a backend's own world, in its own memory | a typed-array view or its bytes |
| a shape converted from buffers | anything the collector owns |

A backend that persists state must still answer a step whose input is the four
buffers, because that is what makes the buffers the interface rather than a
particular engine's object graph. Rebuilding a world from buffers every frame is
allowed and slow; a backend that keeps one must detect that the scene changed
shape — see rule 9.

### 9. A backend REFUSES what it cannot do; it never approximates in silence

The trait in `backend.rs` lets a step answer `Unsupported` and name what it could
not do. This is not politeness, it is the crate's own history: the gather solver
degrades a hull-against-hull pair to a sphere, and that is *correct* here because
`docs/colisores.md` measured the alternative at 2.2 billion dot products a frame —
but it is a silent approximation, and the only reason it is not a trap is that a
document says so.

A third-party backend makes that worse by an order of magnitude. Rapier does
hull-against-hull properly; the gather solver does not; a program written against
one and run on the other would be correct on one machine and wrong on another,
which is exactly the "surface that cannot do what its name means" the workspace
rule refuses.

So: **capability is asked, not assumed.** `Backend::supports` answers before a
step runs, a caller that asks for something a backend lacks gets a refusal by
name, and no backend silently substitutes a shape it does have.

### 10. PARITY is a property of a backend, and it is declared

`RUST × GPU = 0.000000` over 13 supported contacts is measured, and it holds
because the two are the same formulation. **No third-party engine will ever join
that set**, and pretending otherwise is the failure this rule exists to prevent:
Rapier and PhysX are different solvers — different contact generation, different
ordering, different warm-starting — and they will not land where ours lands.

`Backend::parity_group` answers which set a backend belongs to. Two backends in
one group are compared by final position and a divergence is a bug; two in
different groups are not comparable at all, and a test that compares them is
testing nothing.

A backend outside the parity group is not lesser. It is answering a different
question — one with joints and continuous collision in it — and the honest thing
is to say so in the type rather than in a comment nobody reads.

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
