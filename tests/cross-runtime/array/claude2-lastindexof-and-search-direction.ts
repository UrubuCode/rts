// ONE thing: lastIndexOf walks BACKWARDS and clamps its fromIndex differently
// from indexOf — a negative fromIndex past the start makes it find nothing at
// all, where indexOf clamps to 0 and searches everything.
const a = [1, 2, 3, 1, 2, 3];
console.log("idx_default=" + a.indexOf(1) + " last_default=" + a.lastIndexOf(1));
console.log("idx_from3=" + a.indexOf(1, 3) + " last_from3=" + a.lastIndexOf(1, 3));
console.log("idx_from0=" + a.indexOf(3, 0) + " last_from0=" + a.lastIndexOf(3, 0));
console.log("idx_neg1=" + a.indexOf(3, -1) + " last_neg1=" + a.lastIndexOf(3, -1));
console.log("idx_neg99=" + a.indexOf(1, -99) + " last_neg99=" + a.lastIndexOf(1, -99));
console.log("idx_99=" + a.indexOf(1, 99) + " last_99=" + a.lastIndexOf(1, 99));
console.log("idx_nan=" + a.indexOf(1, NaN) + " last_nan=" + a.lastIndexOf(1, NaN));
console.log("idx_undef=" + a.indexOf(1, undefined) + " last_undef=" + a.lastIndexOf(1, undefined));
console.log("idx_frac=" + a.indexOf(2, 1.9) + " last_frac=" + a.lastIndexOf(2, 1.9));
console.log("idx_str=" + a.indexOf(2, "4" as any) + " last_str=" + a.lastIndexOf(2, "4" as any));

// The arity difference is why: lastIndexOf(x) with ONE argument is not the same
// as lastIndexOf(x, undefined) — the second form coerces undefined to 0.
console.log("arity1=" + a.lastIndexOf(3) + " arity2=" + a.lastIndexOf(3, undefined));
console.log("lengths=" + a.indexOf.length + "," + a.lastIndexOf.length);

// Both use STRICT equality, so no coercion and no NaN match.
console.log("strNum=" + [1, 2].indexOf("1" as any) + "," + [1, 2].lastIndexOf("1" as any));
console.log("nan=" + [NaN].indexOf(NaN) + "," + [NaN].lastIndexOf(NaN));
console.log("negZero=" + [-0].indexOf(0) + "," + [0].lastIndexOf(-0));
console.log("objIdentity=" + (() => { const o = {}; return [o].indexOf(o) + "," + [{}].indexOf({} as any); })());

// Both SKIP holes — an explicit undefined is found, a hole is not.
const h: any[] = [undefined, , undefined];
console.log("holes=" + h.indexOf(undefined) + "," + h.lastIndexOf(undefined));
const onlyHole: any[] = [, , ];
console.log("allHoles=" + onlyHole.indexOf(undefined) + "," + onlyHole.lastIndexOf(undefined));

// Empty array answers -1 for every fromIndex.
console.log("empty=" + ([] as number[]).indexOf(1) + "," + ([] as number[]).lastIndexOf(1, 5));

// Generic over an array-like, driven by length.
const like: any = { length: 3, 0: "a", 1: "b", 2: "a" };
console.log("generic=" + Array.prototype.indexOf.call(like, "a") + "," + Array.prototype.lastIndexOf.call(like, "a"));
const shortLen: any = { length: 1, 0: "a", 1: "a" };
console.log("shortLen=" + Array.prototype.lastIndexOf.call(shortLen, "a"));

// includes has no backwards form, and findLast/findLastIndex are the callback
// equivalents — they read holes as undefined where lastIndexOf skips them.
console.log("findLast=" + String(h.findLast((v: any) => v === undefined)) + " findLastIndex=" + h.findLastIndex((v: any) => v === undefined));
let probes = 0;
h.findLastIndex(() => { probes++; return false; });
console.log("findLastProbes=" + probes);

// findLast walks from the END, so a side-effecting predicate shows the order.
const order: number[] = [];
[10, 20, 30].findLast((v) => { order.push(v); return v === 20; });
console.log("findLastOrder=" + order.join(","));
const orderFwd: number[] = [];
[10, 20, 30].find((v) => { orderFwd.push(v); return v === 20; });
console.log("findOrder=" + orderFwd.join(","));
