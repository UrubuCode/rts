// Cross-runtime: `values()`/`keys()`/`entries()` on Array, Map and Set support
// the `.next()` protocol, not just `for-of`/spread.
//
// These used to fail at COMPILE time — "no Registry entry for `Array.next(0
// args)`" — because the result is a materialized array and `Array` has no
// Registry row for `next`. The array stays materialized (so `for-of`, spread,
// `.join()` and `.length` keep working exactly as before); what changed is that
// the handle is REGISTERED as an open iterator when it is created, which makes it
// distinguishable from a plain array at runtime. A plain array must therefore
// keep reading `.next` as `undefined`.

const it = [1, 2].values();
console.log("first=" + it.next().value);
console.log("second=" + it.next().value);
console.log("exhausted=" + JSON.stringify(it.next()));

console.log("keys=" + [7, 8].keys().next().value);
console.log("entries=" + JSON.stringify([9].entries().next().value));

const s = new Set([5, 6]);
console.log("setValues=" + s.values().next().value);
console.log("setKeys=" + s.keys().next().value);

const m = new Map([["k", 9]]);
console.log("mapEntries=" + JSON.stringify(m.entries().next().value));
console.log("mapKeys=" + m.keys().next().value);
console.log("mapValues=" + m.values().next().value);

// Two iterators over the SAME array advance independently — the cursor belongs
// to the iterator, not to the backing array.
const base = [10, 20];
const itA = base.values();
const itB = base.values();
itA.next();
console.log("independent=" + itB.next().value);

// `take(iterator, n)` — the shape bundled code uses.
function take(iter, n) {
  const out: any[] = [];
  for (let i = 0; i < n; i++) {
    const r = iter.next();
    if (r.done) break;
    out.push(r.value);
  }
  return out;
}
console.log("take=" + take([1, 2, 3, 4].values(), 3).join(","));
console.log("takeBeyondEnd=" + take([1, 2].values(), 5).join(","));

// `arr[Symbol.iterator]()` — the CALL. Reading the property already yielded a
// function; calling it went through `__rtsadp_idx_call`, which ToString'd the
// symbol key to "[object Object]", and the native fn's handle return was then
// re-read as a number by the legacy invoker.
const symIt = [1, 2, 3][Symbol.iterator]();
console.log("symIterTypeof=" + typeof symIt);
console.log("symIterFirst=" + symIt.next().value);
console.log("symIterSecond=" + symIt.next().value);
console.log("symIterSpread=" + [...[4, 5][Symbol.iterator]()].join(","));

// ── non-regressions ─────────────────────────────────────────────────────────
console.log("plainArrayNext=" + ([1, 2] as any).next);
console.log("spreadValues=" + [...[1, 2].values()].join(","));
console.log("spreadKeys=" + [...[7, 8].keys()].join(","));

let sum = 0;
for (const v of [1, 2, 3].values()) {
  sum = sum + v;
}
console.log("forOfValues=" + sum);

let pairs = "";
for (const [i, v] of [9, 8].entries()) {
  pairs = pairs + i + ":" + v + " ";
}
console.log("forOfEntries=" + pairs);
