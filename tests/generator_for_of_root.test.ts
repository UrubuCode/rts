// A generator iterated by `for`-`of` can END EARLY, in silence.
//
// # 2026-09-02: two of these five now REPRODUCE it, and that is an improvement
//
// This header used to say the file "does NOT reproduce it" and that "every case
// below passes on the build that has the bug". Both were true when written and
// neither is now. `yield* delegation across the same window` and `an early break
// leaves nothing behind` fail with WRONG ANSWERS — 77994 for 120000, and 1 for
// 20000 — on a plain release binary with no flag set.
//
// Nothing here changed to make that happen. What changed is `rts:test`'s own
// `expect()`, which used to allocate eight times per assertion (a plain object
// plus seven freshly built matcher callables) and now allocates once, through a
// shared prototype. Less allocation between the `for`-`of` and the collection
// stopped hiding the defect. **A test that passes because the harness around it
// is noisy is not passing**, which is why this is recorded as a gain rather than
// papered over: the file has stopped being a guard and started being a
// reproduction.
//
// `docs/engine/lost-roots.md` carries the measurement, the reproducer that works
// outside this harness, and the three candidates excluded so far — including the
// two that were tried on 2026-09-02 and changed nothing (zeroing the generator
// frame, and capturing `xmm6`-`xmm15`).
//
// The original note, kept because the cell it describes is still the shape of
// the thing: what reproduced it in August was a release binary with
// `RTS_GC_DEBUG=1`, which adds one `eprintln!` INSIDE `collect` and changes no
// logic at all:
//     function* g() { yield 1; }
//     let x = 0;
//     for (let i = 0; i < 120000; i++) { for (const v of g()) x = x + v; }
//     console.log("ok", x);
//
//     release + RTS_GC_DEBUG=1   ->  ok 60683      // the loop ended early
//     release, plain             ->  ok 120000
//     debug   + RTS_GC_DEBUG=1   ->  ok 120000
//     debug,   plain             ->  ok 120000
//
// Deterministic in every cell, and one cell is wrong. A flag that only prints
// changing the ANSWER means the value is found by the CONSERVATIVE STACK SCAN
// or not at all — `eprintln!` changes `collect`'s own stack and register layout
// — which is hiding place four of `docs/engine/lost-roots.md`. A root found by
// luck is not a root.
//
// What it is NOT: naming the generator in a local answers 60683 too, so it is
// not "the program failed to hold it"; and rooting `Context::resuming` — the
// obvious candidate, since it is the one field that names a running generator
// and is explicitly not a root — was tried and changed NOTHING in release.
//
// A second, separate defect lives in the same shape: 300 000 rounds EXHAUST the
// heap, before and after that attempt. Two symptoms, at least one cause still
// unfound.

import { describe, test, expect } from "rts:test";

describe("a generator survives the collections its own iteration causes", () => {
  test("for-of over a fresh generator, enough rounds to collect", () => {
    function* one(): Generator<number> {
      yield 1;
    }
    let seen = 0;
    for (let i = 0; i < 60000; i++) {
      for (const v of one()) seen = seen + v;
    }
    expect(seen).toBe(60000);
  });

  test("a generator that yields several times, held in a local", () => {
    function* three(): Generator<number> {
      yield 1;
      yield 2;
      yield 3;
    }
    let total = 0;
    for (let i = 0; i < 20000; i++) {
      const it = three();
      for (const v of it) total = total + v;
    }
    expect(total).toBe(120000);
  });

  test("a generator whose body allocates on every step", () => {
    // Allocation inside the body is what makes a collection land while the
    // frame is live, which is the window the missing root left open.
    function* makes(): Generator<any> {
      yield { a: 1 };
      yield { a: 2 };
    }
    let count = 0;
    for (let i = 0; i < 20000; i++) {
      for (const v of makes()) count = count + v.a;
    }
    expect(count).toBe(60000);
  });

  test("yield* delegation across the same window", () => {
    function* inner(): Generator<number> {
      yield 1;
      yield 2;
    }
    function* outer(): Generator<number> {
      yield* inner();
      yield 3;
    }
    let total = 0;
    for (let i = 0; i < 20000; i++) {
      for (const v of outer()) total = total + v;
    }
    expect(total).toBe(120000);
  });

  test("an early break leaves nothing behind", () => {
    function* many(): Generator<number> {
      yield 1;
      yield 2;
      yield 3;
    }
    let first = 0;
    for (let i = 0; i < 20000; i++) {
      for (const v of many()) {
        first = first + v;
        break;
      }
    }
    expect(first).toBe(20000);
  });
});
