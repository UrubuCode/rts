import { describe, test, expect } from "rts:test";
import { gc } from "rts";

// The periodic GC must trigger on live BYTES, not only on a live-handle count.
//
// `out = out + c` in a loop — what every text-rewriting pass written in .ts does
// — allocates ONE handle per iteration but a payload that grows every time, so N
// iterations retain about N²/2 bytes while holding only N handles. With a
// handle-only floor (500 000) the collector never ran for this shape: 80 000
// concats held 80 k handles, a sixth of the floor, and 3.2 GB of string.
// Measured peak RSS before the byte floor existed: 288 MB at 20 k concats,
// 984 MB at 40 k, 3 644 MB at 80 k. The same program with an explicit
// `gc.collect` every 2 000 iterations peaked at 93 MB — the collector was right,
// the trigger was counting the wrong thing.
//
// This asserts the OBSERVABLE consequence rather than a byte number (which is a
// tuning constant and would make the test a change-detector): after building a
// large string, the live-handle count must have come back down, i.e. the
// intermediates were reclaimed WITHOUT the program asking. A run where the GC
// never fires leaves every intermediate live.

const N = 40000;

const before: i64 = gc.live_count();

let out = "";
let i = 0;
while (i < N) {
  out = out + "x";
  i = i + 1;
}

const after: i64 = gc.live_count();
const grew: i64 = after - before;

describe("GC triggers on live bytes, not just handle count", () => {
  test("the built string is correct", () => {
    expect(out.length).toBe(N);
  });

  test("intermediates were reclaimed without an explicit collect", () => {
    // Every intermediate retained would mean ~N live handles more than we
    // started with. Reclaiming leaves a small residue. The bound is deliberately
    // loose — it separates "the collector ran" from "it never ran", which is the
    // property under test, and does not pin a tuning constant.
    expect(grew < N / 2).toBe(true);
  });
});
