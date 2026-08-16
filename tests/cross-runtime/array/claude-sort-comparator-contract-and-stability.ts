// ONE thing: what the default comparator actually does (ToString, then UTF-16
// code-unit order) and what a custom one is allowed to return.
const nums = [10, 9, 1, 100, 2, 20];
console.log("default=" + nums.slice().sort().join(","));
console.log("numeric=" + nums.slice().sort((a, b) => a - b).join(","));

// ToString happens on every element, so an object sorts by its toString.
const objs: any[] = [{ toString: () => "b" }, { toString: () => "a" }];
console.log("byToString=" + objs.slice().sort().join(","));

// Code-unit order, not locale: uppercase before lowercase, digits before both.
console.log("caseOrder=" + ["b", "A", "a", "B", "1", "_"].sort().join(""));
console.log("unicode=" + ["é", "z", "a"].sort().join(""));

// A comparator returning a non-number is coerced; NaN and undefined mean "keep".
const weird = [3, 1, 2];
console.log("nanCmp=" + weird.slice().sort(() => NaN).join(","));
console.log("undefCmp=" + weird.slice().sort(() => undefined as any).join(","));
// A boolean comparator is deliberately absent: `a > b` never expresses "less
// than", so the comparator is inconsistent and the ORDER it produces is
// implementation-defined — Bun and Node genuinely disagree on it.
console.log("strCmp=" + weird.slice().sort((a, b) => String(a - b) as any).join(","));
console.log("fracCmp=" + weird.slice().sort((a, b) => (a - b) * 0.0001).join(","));
console.log("negZeroCmp=" + weird.slice().sort(() => -0).join(","));

// Stability is REQUIRED since ES2019: equal keys keep their input order.
const recs = [
  { k: 1, id: "a" }, { k: 0, id: "b" }, { k: 1, id: "c" },
  { k: 0, id: "d" }, { k: 1, id: "e" }, { k: 0, id: "f" },
  { k: 1, id: "g" }, { k: 0, id: "h" }, { k: 1, id: "i" },
  { k: 0, id: "j" }, { k: 1, id: "k" }, { k: 0, id: "l" },
];
console.log("stable=" + recs.slice().sort((x, y) => x.k - y.k).map((r) => r.id).join(""));

// The comparator is not called at all below two elements.
let calls = 0;
[1].sort(() => { calls++; return 0; });
([] as number[]).sort(() => { calls++; return 0; });
console.log("smallCalls=" + calls);

// sort returns the SAME array; toSorted returns a new one and leaves the source.
const src = [3, 1, 2];
console.log("sortIdentity=" + (src.sort() === src));
const src2 = [3, 1, 2];
const out = src2.toSorted();
console.log("toSortedNew=" + (out !== src2) + " src=" + src2.join(",") + " out=" + out.join(","));

// A non-callable, non-undefined comparator is a TypeError before any work.
try { ([2, 1] as any).sort(null); } catch (e: any) { console.log("nullCmp=" + e.constructor.name); }
try { ([2, 1] as any).sort(5); } catch (e: any) { console.log("numCmp=" + e.constructor.name); }
console.log("explicitUndef=" + [2, 1].sort(undefined).join(","));
