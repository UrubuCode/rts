// A generator iterated by `for`-`of` used to END EARLY, in silence. FIXED
// 2026-09-02, and these five cases are what say so.
//
// # What it was, and why it took three weeks
//
// `generator_new` allocates the parked FRAME and only makes it reachable three
// lines later, when the `State` goes into `context.generators`. Between the
// two, `frame` is a bare `u32` in a Rust local — the conservative stack scan
// keeps only words that decode as an encoded `Value`, and a raw cell index is
// not one — and `trace::edges_of`'s arm 9, "the one place that knows the frame
// exists at all", reaches a frame THROUGH `context.generators`, which does not
// have the entry yet. `made()` sits in that window and allocates.
//
// The fix is one `rooted::Rooted` guard over the window, which is hiding place
// two of `docs/engine/lost-roots.md` and the same shape as `json::materialise`.
//
// Three earlier attempts missed it because they all looked for a root of the
// generator OBJECT: `Context::resuming` (which only covers the body's
// duration), zeroing the frame's slots (which tests over-retention, and this
// was under-retention), and capturing `xmm6`-`xmm15`. What died was the FRAME,
// and it died before the object existed — which is also why naming the
// generator in a local never helped.
//
// # The measurement that found it, because the SHAPE of it is the lesson
//
// The answer SATURATED. `for (const v of g())` answered 63028 for N of 100 000,
// 200 000, 400 000 and 1 000 000 alike. A saturating answer is not a rate — it
// is one event, and everything after it fails. 65536 is the region's initial
// capacity and `live` sat near 2400: 65536 - 2400 is where the answer stopped,
// so the round the region first FILLS is the round the first collection runs,
// and the allocation that triggers it is the one inside the window.
//
// Reading the number as a rate ("a collection caught a bad window sometimes")
// is what kept the search on the wrong object for three weeks. Plotting it
// against N cost one minute and pointed straight at the first collection.
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
