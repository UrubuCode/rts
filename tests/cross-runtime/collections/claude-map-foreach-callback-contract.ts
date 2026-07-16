// Cross-runtime: Map/Set forEach callback contract — the (value, key, collection)
// argument triple, thisArg, return value, and behaviour under mutation.

// --- argument triple ---
const m = new Map([["a", 1], ["b", 2]]);
const rows: string[] = [];
m.forEach(function (value, key, coll) {
  rows.push(key + "=" + value + ":same_map=" + (coll === m) + ":args=" + arguments.length);
});
console.log("triple=" + rows.join("|"));

// --- value comes FIRST, key second (the classic trap) ---
const order: string[] = [];
new Map([["k", "v"]]).forEach((a, b) => order.push("first=" + a + ",second=" + b));
console.log("arg_order=" + order.join(""));

// --- forEach returns undefined ---
console.log("returns=" + String(m.forEach(() => 1)));

// --- thisArg (2nd param) ---
const ctx = { tag: "CTX", hits: [] as string[] };
new Map([["x", 1], ["y", 2]]).forEach(function (this: any, v, k) {
  this.hits.push(this.tag + ":" + k + v);
}, ctx);
console.log("thisArg=" + ctx.hits.join("|"));

// --- arrow ignores thisArg (lexical this) ---
const outer = { tag: "OUTER" };
let arrowSaw = "";
(function (this: any) {
  new Map([["z", 9]]).forEach(() => { arrowSaw = this.tag; }, { tag: "PASSED" });
}).call(outer);
console.log("arrow_ignores_thisArg=" + arrowSaw);

// (Note: `this` inside a callback with no thisArg is intentionally NOT probed —
// it depends on module vs CommonJS strictness, so the runtimes legitimately
// disagree and it says nothing about Map.forEach itself.)

// --- empty map never calls back ---
let calls = 0;
new Map().forEach(() => { calls++; });
console.log("empty_calls=" + calls);

// --- callback fires once per entry, in insertion order ---
const m2 = new Map([["a", 1], ["b", 2], ["c", 3]]);
let n = 0;
const seq: string[] = [];
m2.forEach((v, k) => { n++; seq.push(k); });
console.log("count=" + n + ":seq=" + seq.join(","));

// --- entries added DURING forEach are visited ---
const m3 = new Map([["a", 1]]);
const visited: string[] = [];
m3.forEach((v, k) => {
  visited.push(k);
  if (k === "a") m3.set("b", 2);
});
console.log("added_during=" + visited.join(","));

// --- entries deleted DURING forEach are skipped ---
const m4 = new Map([["a", 1], ["b", 2], ["c", 3]]);
const visited2: string[] = [];
m4.forEach((v, k) => {
  visited2.push(k);
  if (k === "a") m4.delete("b");
});
console.log("deleted_during=" + visited2.join(","));

// --- value seen is the CURRENT value, not the value at forEach start ---
const m5 = new Map([["a", 1], ["b", 2]]);
const vals: string[] = [];
m5.forEach((v, k) => {
  vals.push(k + "=" + v);
  if (k === "a") m5.set("b", 99);
});
console.log("live_values=" + vals.join(","));

// --- delete + re-add during forEach re-visits at the end ---
const m6 = new Map([["a", 1], ["b", 2]]);
const visited3: string[] = [];
m6.forEach((v, k) => {
  visited3.push(k);
  if (k === "a" && visited3.length === 1) { m6.delete("a"); m6.set("a", 1); }
});
console.log("readd_during=" + visited3.join(","));

// --- exception propagates out of forEach ---
try {
  new Map([["a", 1], ["b", 2]]).forEach((v, k) => { if (k === "b") throw new Error("stop"); });
} catch (e: any) {
  console.log("throws=" + e.message);
}

// --- Set.forEach: value passed TWICE (value, value, set) ---
const s = new Set(["p", "q"]);
const srows: string[] = [];
s.forEach(function (v1, v2, coll) {
  srows.push(v1 + ":" + (v1 === v2) + ":same_set=" + (coll === s) + ":args=" + arguments.length);
});
console.log("set_triple=" + srows.join("|"));

const sctx = { n: 0 };
new Set([1, 2, 3]).forEach(function (this: any, v) { this.n += v; }, sctx);
console.log("set_thisArg=" + sctx.n);
console.log("set_returns=" + String(new Set([1]).forEach(() => 1)));
