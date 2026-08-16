// ONE thing: how slice and at turn their arguments into indices. Both use
// ToIntegerOrInfinity, which truncates toward zero and maps NaN to 0 — a rule
// that makes several surprising pairs equal.
const a = [0, 1, 2, 3, 4];
function sl(label: string, ...args: any[]) {
  console.log(label + "=" + (a.slice as any)(...args).join(",") + "|");
}
sl("none");
sl("zero", 0);
sl("two", 2);
sl("neg2", -2);
sl("neg99", -99);
sl("pos99", 99);
sl("nan", NaN);
sl("undef", undefined);
sl("null", null);
sl("true", true);
sl("frac", 1.9);
sl("negFrac", -1.9);
sl("str", "2");
sl("strNeg", "-2");
sl("strJunk", "abc");
sl("emptyStr", "");
sl("range", 1, 3);
sl("negRange", -3, -1);
sl("crossed", 3, 1);
sl("endUndef", 1, undefined);
sl("endNull", 1, null);
sl("inf", -Infinity, Infinity);

// at() maps a fractional or NaN index the same way.
console.log("at0=" + String(a.at(0)) + " atNeg1=" + String(a.at(-1)) + " atNeg99=" + String(a.at(-99)));
console.log("atFrac=" + String(a.at(1.9)) + " atNegFrac=" + String(a.at(-1.9)));
console.log("atNaN=" + String(a.at(NaN)) + " atUndef=" + String(a.at(undefined as any)));
console.log("atStr=" + String(a.at("2" as any)) + " atTrue=" + String(a.at(true as any)));
console.log("atNegZero=" + String(a.at(-0)) + " atLen=" + String(a.at(5)));

// An object argument goes through ToPrimitive first.
const box = { valueOf() { return 2; } };
console.log("objIndex=" + a.slice(box as any).join(",") + " at=" + String(a.at(box as any)));
const boxStr = { toString() { return "-2"; } };
console.log("objStrIndex=" + a.slice(boxStr as any).join(","));

// The coercion happens ONCE per argument, in order.
const order: string[] = [];
const s1 = { valueOf() { order.push("start"); return 1; } };
const e1 = { valueOf() { order.push("end"); return 3; } };
a.slice(s1 as any, e1 as any);
console.log("coercionOrder=" + order.join(","));

// slice preserves holes; at reads them as undefined.
const h: any[] = [0, , 2];
const cut = h.slice(0, 3);
console.log("sliceHoles=" + [0, 1, 2].map((i) => (i in cut ? "y" : "n")).join("") + " at1=" + String(h.at(1)));

// slice is generic and always answers a plain Array for an array-like.
const like: any = { length: 3, 0: "a", 2: "c" };
const g = Array.prototype.slice.call(like, 0);
console.log("genericIsArray=" + Array.isArray(g) + " in=" + [0, 1, 2].map((i) => (i in g ? "y" : "n")).join(""));
console.log("genericAt=" + String(Array.prototype.at.call(like, -1)));

// A throwing valueOf aborts before any element is read.
let read = 0;
const watched: any = new Proxy([1, 2, 3], { get(t: any, k) { if (typeof k === "string" && /^\d+$/.test(k)) read++; return t[k]; } });
try { Array.prototype.slice.call(watched, { valueOf() { throw new RangeError("stop"); } } as any); }
catch (e: any) { console.log("abort=" + e.constructor.name + " elementsRead=" + read); }
