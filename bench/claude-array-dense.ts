// Dense-array fast paths of Array.prototype: indexOf, includes, slice, push,
// lastIndexOf, fill, join. Written 2026-09-26 for the entry-tax claim of the
// generic-arm lot (array_proto/generic.rs): a real dense Array must still be
// answered in one borrow. Runs unmodified under rts, node and bun. The number
// that matters is the same binary before and after, release, not the runtime
// comparison.
const N = 200_000;
const a: number[] = [];
for (let i = 0; i < N; i++) a.push(i);

function time(name: string, f: () => number) {
  const t0 = Date.now();
  const r = f();
  const dt = Date.now() - t0;
  console.log(name + " " + dt + " ms (" + r + ")");
}

time("indexOf", () => { let s = 0; for (let k = 0; k < 200; k++) s += a.indexOf(N - 1 - (k & 7)); return s; });
time("includes", () => { let s = 0; for (let k = 0; k < 200; k++) s += a.includes(N - 1 - (k & 7)) ? 1 : 0; return s; });
time("lastIndexOf", () => { let s = 0; for (let k = 0; k < 200; k++) s += a.lastIndexOf(k & 7); return s; });
time("slice", () => { let s = 0; for (let k = 0; k < 200; k++) s += a.slice(k, k + 50_000).length; return s; });
time("push", () => { const b: number[] = []; for (let i = 0; i < 2_000_000; i++) b.push(i); return b.length; });
time("fill", () => { let s = 0; for (let k = 0; k < 200; k++) { a.fill(k, 0, 100_000); s += a[0]; } return s; });
time("join", () => { let s = 0; for (let k = 0; k < 20; k++) s += a.join(",").length; return s; });
