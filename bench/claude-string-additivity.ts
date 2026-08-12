// Is the string method call the SUM of an uncached get and a call?
//
// Counted (bench/claude-string-counts.ts): `s.m(x)` makes exactly one uncached
// get and exactly one call per iteration, the same one each that the two rows
// below make separately. If the cost were the sum, the third row would be the
// first two added. It is the falsifier for "the get and the call each cost
// what they cost".
type Case = { group: string; name: string; ops: number; run: (n: number) => number };
const CASES: Case[] = [];
let SINK = 0;
function bench(group: string, name: string, run: (n: number) => number): void {
  CASES.push({ group, name, ops: 1, run: run });
}
bench("floor", "empty loop", (n) => { let a = 0; for (let i = 0; i < n; i++) a += i; return a; });

const s16 = "abcdefghijklmnop";
const p = s16 as never as { __step0: (i: number) => number };
const g = p.__step0;

bench("X", "get only: m = p.__step0", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = p.__step0;
  return m === undefined ? 1 : 2;
});
bench("X", "call only: g(i)", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += g(i & 15);
  return a;
});
bench("X", "both, separately, same loop", (n) => {
  let a = 0;
  let m: unknown = 0;
  for (let i = 0; i < n; i++) { m = p.__step0; a += g(i & 15); }
  return a + (m === undefined ? 1 : 0);
});
bench("X", "together: p.__step0(i)", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step0(i & 15);
  return a;
});
bench("X", "get the result and call IT: (p.__step0)(i)", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) { const f = p.__step0; a += f(i & 15); }
  return a;
});

type Row = { group: string; name: string; nanos: number; failed: string };
const TARGET_MS = 120;
function timeOnce(c: Case, n: number): number { const t0 = Date.now(); SINK += c.run(n); return Date.now() - t0; }
function measure(c: Case): Row {
  try {
    let n = 1024; let ms = timeOnce(c, n);
    while (ms < TARGET_MS && n < 1 << 26) { n = n * 4; ms = timeOnce(c, n); }
    for (let w = 0; w < 2; w++) ms = timeOnce(c, n);
    let best = ms;
    for (let r = 0; r < 2; r++) { const again = timeOnce(c, n); if (again < best) best = again; }
    return { group: c.group, name: c.name, nanos: (best * 1e6) / n, failed: "" };
  } catch (e) { return { group: c.group, name: c.name, nanos: 0, failed: String(e).slice(0, 60) }; }
}
function pad(s: string, w: number): string { let o = s; while (o.length < w) o = o + " "; return o; }
const rows: Row[] = [];
for (const c of CASES) rows.push(measure(c));
const floor = rows[0].nanos;
for (const r of rows) console.log(pad(r.group + " " + r.name, 46) + (r.failed !== "" ? "UNAVAILABLE " + r.failed : (r.nanos - floor).toFixed(2)));
console.log("floor " + floor.toFixed(2) + "  checksum " + SINK);
