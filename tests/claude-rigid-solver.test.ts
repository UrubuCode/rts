// `rts:rigid` — the parallel rigid-body solver, reached from a program.
//
// What this pins is the SURFACE, not the physics: the physics is pinned by the
// Rust tests in `crates/rts-physics/src/solver/tests.rs`, which can name a
// constant and a branch. What only a program can check is that four typed arrays
// cross the boundary, come back written in place, and that a call the solver
// cannot honour answers zero instead of half-simulating.
import { describe, test, expect } from "rts:test";
import { step, threads } from "rts:rigid";

// pos: centre + sleep counter. vel: velocity + shape (0 sphere, 1 box).
// ext: half-extent + inverse mass. world: dt, static count, cell size, sub-steps,
// then one static box as (centre, half-extent).
function floor(substeps: number): Float32Array {
  const world = new Float32Array(12);
  world[0] = 1 / 60;
  world[1] = 1;
  world[2] = 1;
  world[3] = substeps;
  world[5] = -1;
  world[8] = 200;
  world[9] = 0.5;
  world[10] = 200;
  return world;
}

describe("rts:rigid", () => {
  test("a body dropped on a floor comes to rest on top of it", () => {
    const pos = new Float32Array([0, 6, 0, 0]);
    const vel = new Float32Array([0, 0, 0, 1]);
    const ext = new Float32Array([0.5, 0.5, 0.5, 1]);
    const moved = step(pos, vel, ext, floor(240));

    expect(moved).toBe(1);
    // The floor's top is y = -0.5, so a half-extent of 0.5 rests at y = 0, less
    // the slop the solver leaves uncorrected on purpose.
    expect(pos[1] > -0.1 && pos[1] < 0.1).toBe(true);
    expect(Math.abs(vel[1]) < 0.5).toBe(true);
  });

  test("the buffers are written in place rather than answered as copies", () => {
    // The whole reason the surface takes typed arrays: a solver that answered
    // new arrays would cost a copy per frame and the caller would still hold the
    // old ones.
    const pos = new Float32Array([0, 40, 0, 0]);
    const vel = new Float32Array([0, 0, 0, 0]);
    const ext = new Float32Array([0.5, 0.5, 0.5, 1]);
    step(pos, vel, ext, floor(30));
    expect(pos[1] < 40).toBe(true);
    expect(vel[1] < 0).toBe(true);
  });

  test("the shape survives a step, so a sphere does not become a box", () => {
    const pos = new Float32Array([0, 20, 0, 0]);
    const vel = new Float32Array([0, 0, 0, 0]);
    const ext = new Float32Array([0.5, 0.5, 0.5, 1]);
    step(pos, vel, ext, floor(10));
    expect(vel[3]).toBe(0);
  });

  test("two views over one buffer are refused instead of aliased", () => {
    // Legal JavaScript, and unsound for the solver: two of the four arguments
    // become mutable slices of the same bytes. Answering zero is the refusal.
    const shared = new ArrayBuffer(64);
    const pos = new Float32Array(shared, 0, 4);
    const vel = new Float32Array(shared, 0, 4);
    const ext = new Float32Array([0.5, 0.5, 0.5, 1]);
    expect(step(pos, vel, ext, floor(1))).toBe(0);
  });

  test("something that is not a typed array is refused", () => {
    const ext = new Float32Array([0.5, 0.5, 0.5, 1]);
    expect(step([0, 1, 0, 0], new Float32Array(4), ext, floor(1))).toBe(0);
  });

  test("the solver says how many threads it spreads a step over", () => {
    // A performance number that does not say how many threads produced it is
    // not a number, so the program measuring one can ask.
    expect(threads() >= 1).toBe(true);
  });
});
