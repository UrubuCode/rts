// `charCodeAt` cut into prefixes - what each step inside the native costs.
//
// Requires the probe natives on `String.prototype` (branch
// `claude/string-cost-probe`, `crates/rts-core/src/entry/string/probe.rs`).
// Without them every step row reads UNAVAILABLE, which is the point: a missing
// probe reports itself rather than reporting a number for nothing.
//
// Read the rows as DIFFERENCES. Each `__stepN` is `charCodeAt` truncated one
// step earlier, so `__step2 - __step1` is `length_of` and nothing else.

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
const p = s16 as never as {
  __step0: (i: number) => number;
  __step1: (i: number) => number;
  __step2: (i: number) => number;
  __step3: (i: number) => number;
  __step4: (i: number) => number;
};
const obj = { m: (x: number) => x };

bench("S", "0 call only", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step0(i & 15);
  return a;
});
bench("S", "1 + with_current", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step1(i & 15);
  return a;
});
bench("S", "2 + length_of", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step2(i & 15);
  return a;
});
bench("S", "3 + argument decode", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step3(i & 15);
  return a;
});
bench("S", "4 + indexed (= charCodeAt)", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step4(i & 15);
  return a;
});
bench("S", "real charCodeAt", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.charCodeAt(i & 15);
  return a;
});
bench("S", "read s.__step0, no call", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = p.__step0;
  return m === undefined ? 1 : 2;
});
bench("S", "obj.m(x) - same call, shaped receiver", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += obj.m(i & 15);
  return a;
});
bench("S", "Math.abs(x) - native, shaped receiver", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.abs(i & 15);
  return a;
});

const holder = { n: p.__step0, m2: (x: number) => x };
const f0 = p.__step0;

bench("T", "p.__step0(x) - string receiver", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += p.__step0(i & 15);
  return a;
});
bench("T", "holder.n(x) - SAME native, shaped receiver", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += holder.n(i & 15);
  return a;
});
bench("T", "f0(x) - SAME native, plain call", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += f0(i & 15);
  return a;
});
bench("T", "holder.m2(x) - JS fn, shaped receiver", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += holder.m2(i & 15);
  return a;
});
bench("T", "read p.__step0 (uncached get on string)", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = p.__step0;
  return m === undefined ? 1 : 2;
});
bench("T", "read holder.n (cached get)", (n) => {
  let m: unknown = 0;
  for (let i = 0; i < n; i++) m = holder.n;
  return m === undefined ? 1 : 2;
});

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
console.log("step                                            ns/op    minus floor");
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
console.log("floor: " + floor.toFixed(2) + " ns   checksum " + SINK);
