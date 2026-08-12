// Where the ~400 ns of `charCodeAt` go, decomposed.
//
// # Why a second file rather than more rows in `analytic.ts`
//
// The rows here are DIFFERENCES between cases that share everything but one
// step, and that only means anything if the cases were measured in one process
// against one floor. `analytic.ts` measures the actions; this measures the
// steps inside three of them.
//
// The three controls at the top of each group are copied verbatim from
// `analytic.ts` so a reader can check that this file's floor and its harness
// agree with that one's before believing any difference below them.
//
// # What it does not say
//
// Nothing about a program: every case is one action in a loop with its operands
// in hand. And a difference between two rows is an attribution only where the
// two differ in exactly one step — which is why each pair is written adjacent
// and the step is named in the case's name.

type Case = { group: string; name: string; ops: number; run: (n: number) => number };

const CASES: Case[] = [];
let SINK = 0;

function bench(group: string, name: string, run: (n: number) => number, ops?: number): void {
  CASES.push({ group, name, ops: ops === undefined ? 1 : ops, run: run });
}

bench("floor", "empty loop", (n) => {
  let acc = 0;
  for (let i = 0; i < n; i++) acc += i;
  return acc;
});

const s16 = "abcdefghijklmnop";
const s256 = s16.repeat(16);
const s4096 = s16.repeat(256);
const sx = "xy";
const obj = { m: (x: number) => x, v: 7 };

// --------------------------------------------------------------- controls

bench("A control", "length", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.length;
  return a;
});
bench("A control", "charCodeAt", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.charCodeAt(i & 15);
  return a;
});
bench("A control", "index [] + length", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16[i & 15].length;
  return a;
});
bench("A control", "concat 2 + length", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (s16 + "x").length;
  return a;
});
bench("A control", "template + length", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += `v=${i}!`.length;
  return a;
});

// ------------------------------ B: read the method WITHOUT calling it
// The difference B(read) - A(length) is what a prototype-chain walk costs over
// a direct string property; B(call) - B(read) is what the call costs.

bench("B read", "s.charCodeAt (no call)", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = s16.charCodeAt;
  return m === undefined ? 1 : 2;
});
bench("B read", "s.length (no add)", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = s16.length;
  return m === undefined ? 1 : 2;
});
bench("B read", "s.nosuch (absent, whole chain)", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = (s16 as never as { nosuch: number }).nosuch;
  return m === undefined ? 1 : 2;
});

// ------------------------------ C: does charCodeAt cost grow with LENGTH?
// If it copies the receiver, 4096 units costs 256x what 16 does. If it does
// not, the three rows are the same number. This is the falsifier for the
// claim "charCodeAt is the one string method that does not copy".

bench("C length", "charCodeAt on 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.charCodeAt(i & 15);
  return a;
});
bench("C length", "charCodeAt on 256", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s256.charCodeAt(i & 15);
  return a;
});
bench("C length", "charCodeAt on 4096", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s4096.charCodeAt(i & 15);
  return a;
});
bench("C length", "slice(0,1) on 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.slice(0, 1).length;
  return a;
});
bench("C length", "slice(0,1) on 4096", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s4096.slice(0, 1).length;
  return a;
});

// ------------------------------ D: is the cost the CALL, or the STRING?
// Same shape of call, receiver that has a shape and an inline cache.

bench("D callee", "obj.m(x) — JS method, 1 arg", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += obj.m(i & 15);
  return a;
});
bench("D callee", "obj.v — shaped property", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += obj.v;
  return a;
});
bench("D callee", "Math.abs(x) — native, 1 arg", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.abs(i & 15);
  return a;
});
bench("D callee", "s.charCodeAt(x) — native on string", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.charCodeAt(i & 15);
  return a;
});

// ------------------------------ E: index [] with and without the .length

bench("E index", "s[i] alone", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = s16[i & 15];
  return t.length;
});
bench("E index", "s[i].length", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16[i & 15].length;
  return a;
});
bench("E index", "s.charAt(i) alone", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = s16.charAt(i & 15);
  return t.length;
});

// ------------------------------ F: template vs concat, and number-to-string

bench("F build", "s16 + 'x' alone", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = s16 + "x";
  return t.length;
});
bench("F build", "`v=${sx}!` — 3 parts, all strings", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = `v=${sx}!`;
  return t.length;
});
bench("F build", "'v=' + sx + '!' — same, as concat", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = "v=" + sx + "!";
  return t.length;
});
bench("F build", "`v=${i}!` — 3 parts, one number", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = `v=${i}!`;
  return t.length;
});
bench("F build", "'v=' + i + '!' — same, as concat", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = "v=" + i + "!";
  return t.length;
});
bench("F build", "'' + i — number to string alone", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = "" + i;
  return t.length;
});
bench("F build", "'' + sx — string to string alone", (n) => {
  let t = "";
  for (let i = 0; i < n; i++) t = "" + sx;
  return t.length;
});

// ------------------------------------------------------------- the harness
// Copied from `analytic.ts` unchanged, so the two files' numbers are
// comparable rather than merely similar.

type Row = { group: string; name: string; nanos: number; failed: string };

const TARGET_MS = 120;
const WARMUP = 2;

function timeOnce(c: Case, n: number): number {
  const t0 = Date.now();
  SINK += c.run(n);
  return Date.now() - t0;
}

function measure(c: Case): Row {
  try {
    let n = 1024;
    let ms = timeOnce(c, n);
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
    return { group: c.group, name: c.name, nanos: (best * 1e6) / (n * c.ops), failed: "" };
  } catch (e) {
    return { group: c.group, name: c.name, nanos: 0, failed: String(e).slice(0, 60) };
  }
}

function pad(s: string, w: number): string {
  let out = s;
  while (out.length < w) out = out + " ";
  return out;
}

function padLeft(s: string, w: number): string {
  let out = s;
  while (out.length < w) out = " " + out;
  return out;
}

const rows: Row[] = [];
for (const c of CASES) rows.push(measure(c));

const floor = rows[0].nanos;
console.log("action                                          ns/op    minus floor");
console.log("------------------------------------------------------------------");
for (const r of rows) {
  const label = pad(r.group + " " + r.name, 46);
  if (r.failed !== "") {
    console.log(label + "  UNAVAILABLE  " + r.failed);
    continue;
  }
  const net = r.nanos - floor;
  console.log(label + padLeft(r.nanos.toFixed(2), 9) + padLeft(net > 0 ? net.toFixed(2) : "~0", 13));
}
console.log("------------------------------------------------------------------");
console.log("floor (empty loop iteration): " + floor.toFixed(2) + " ns");
console.log("checksum " + SINK);
