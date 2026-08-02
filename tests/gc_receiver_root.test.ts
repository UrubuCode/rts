import { describe, test, expect } from "rts:test";

// RTS_OPTIMIZATION.md §11.2. The engine used to mask the receiver word down to
// its 48-bit payload AT THE CALL SITE before every fused heap access. The
// conservative stack scanner recognizes a root by its non-zero handle
// GENERATION — the top 16 bits — which is exactly what that mask clears, so
// once the (pure, loop-invariant) mask was hoisted out of a loop, the only word
// still live across the loop was one the scanner is structurally unable to see.
// The receiver's slot was swept while the program was still using it.
//
// The failure is a WRONG ANSWER, not a crash: a PolyValue carries only a slot
// index, so the stale word silently resolved to whatever object landed in the
// reused slot.
//
// The 600 000 size cannot shrink. Collection only starts once the live set
// passes GC_LIVE_FLOOR (500 000 handles), so a smaller fixture never collects
// at all and would pass on the broken engine — the same reason
// `gc_large_live_set.test.ts` carries its size.

class N {
  v: number;
  constructor(v: number) {
    this.v = v;
  }
}

// (a) a binding whose ONLY use after the allocating loop is a field read. This
// is the exact shape that lost its root: with only `plain.v` downstream, the
// masked payload is the sole live form of the receiver.
const plain = new N(7);
const junk: N[] = [];
for (let i = 0; i < 600000; i++) junk.push(new N(i));
const plainAfter = plain.v;
const junkLen = junk.length;

// (b) the control that always passed, kept so a regression tells the two apart:
// `typeof` forces the whole boxed word to stay live, which the scanner can see.
const boxedAlive = new N(11);
const junk2: N[] = [];
for (let i = 0; i < 600000; i++) junk2.push(new N(i));
const boxedType = typeof boxedAlive;
const boxedAfter = boxedAlive.v;

describe("gc: a receiver used only through a field read stays rooted", () => {
  test("field read after a collecting loop returns the original value", () => {
    expect(plainAfter).toBe(7);
  });

  test("the loop really did allocate past the collection floor", () => {
    expect(junkLen).toBe(600000);
  });

  test("the boxed-word control is unaffected", () => {
    expect(boxedType).toBe("object");
    expect(boxedAfter).toBe(11);
  });
});
