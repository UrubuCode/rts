// What each action costs, in nanoseconds, measured the same way on every
// runtime that can run this file.
//
// # Why one file and no imports
//
// It is run under `rts`, under `node` and under `bun`, and the number that
// means anything is the RATIO between them: "string concatenation costs 40 ns"
// says nothing on its own, while "40 ns here and 6 ns on node" is a work item.
// An import would tie the measurement to one runtime's module resolution, which
// is one of the things being measured.
//
// # What it does not say
//
// Nothing about a program. Every case here runs one action in a loop with its
// operands already in hand, which is the best case for caches and the worst
// case for representativeness — a case that is fast here can still be slow in a
// program that reaches it once. It is an attribution instrument: it says which
// actions are expensive, not how much of any given program they are.
//
// # A known defect in this harness — read before quoting a number from it
//
// Several rows report roughly TEN TIMES what the same operation costs when it
// is written on its own. Measured 2026-08-11, release: `obj.a` in a loop is
// 225 ns here and 16 ns in a four-line file; `typeof obj` is 250 ns here and
// 26 ns there. The cause is not known yet, and four candidates have been
// ruled out by measurement rather than by argument — reading an outer-scope
// variable costs 3 ns more than a local, repeating the same variable names
// across 40 closures costs nothing, storing a closure in an object costs
// nothing, and the throw check is 9%.
//
// So: the RANKING within a column is usable, and the ABSOLUTE nanoseconds are
// not, and neither is any ratio to node computed from them. What this file is
// good for until that is found is finding the expensive shapes and the
// unavailable operations. Whoever finds the cause should delete this paragraph
// and say what it was.
//
// # Honesty
//
// Every case returns a number derived from its work and the harness sums it,
// so an optimiser removing the body shows up as a number too good to be true
// rather than as a fast one. The empty case measures the loop itself and is
// reported as the floor: a case at the floor is one whose cost the harness
// dominates, not a free action.

type Case = { group: string; name: string; ops: number; run: (n: number) => number };

const CASES: Case[] = [];
let SINK = 0;

function bench(group: string, name: string, run: (n: number) => number, ops?: number): void {
  CASES.push({ group, name, ops: ops === undefined ? 1 : ops, run: run });
}

// ---------------------------------------------------------------- the floor

bench("floor", "empty loop", (n) => {
  let acc = 0;
  for (let i = 0; i < n; i++) acc += i;
  return acc;
});

const obj = { a: 1, b: 2, c: 3, d: 4 };
bench("prop", "read own", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += obj.a;
  return a;
});
// ------------------------------------------------------------------ harness

// Nanoseconds a case costs per action, with the loop that carried it removed.
//
// The count is CALIBRATED rather than fixed: a case at 2 ns and a case at 2 us
// share this harness, and one iteration count cannot serve both — too few and
// the clock's own resolution is the number, too many and the slow cases take
// minutes. So each case is grown until it takes at least `TARGET_MS`.
const TARGET_MS = 40;
const WARMUP = 1;

function timeOnce(c: Case, n: number): number {
  const t0 = performance.now();
  SINK += c.run(n);
  return performance.now() - t0;
}

type Row = { group: string; name: string; nanos: number; failed: string };

function measure(c: Case): Row {
  if (true) {
    let n = 1024;
    let ms = timeOnce(c, n);
    // Growing by a factor rather than by extrapolation: a case whose cost is
    // not linear in `n` (an array that grows, a string that accumulates) would
    // have the extrapolation overshoot by orders of magnitude.
    while (ms < TARGET_MS && n < 1 << 26) {
      n = n * 4;
      ms = timeOnce(c, n);
    }
    for (let w = 0; w < WARMUP; w++) ms = timeOnce(c, n);
    let best = ms;
    for (let r = 0; r < 2; r++) {
      const again = timeOnce(c, n);
      if (again < best) best = again;
    }
    console.log("DEBUG " + c.name + " n=" + n + " best=" + best);
    return { group: c.group, name: c.name, nanos: (best * 1e6) / (n * c.ops), failed: "" };
  } return { group: c.group, name: c.name, nanos: 0, failed: "x" };
}
for (const c of CASES) { const r = measure(c); console.log(r.name + " " + r.nanos.toFixed(2)); }
