// ONE thing: which property keys count as ARRAY INDICES. Only the canonical
// decimal form of an integer in [0, 2^32-2) updates length; everything else is
// an ordinary string key that sits beside the elements.
const a: any = [];
a[0] = "zero";
a["1"] = "one";
a["01"] = "not-index";
a["1.0"] = "not-index";
a[" 2"] = "not-index";
a["-0"] = "not-index";
a["1e2"] = "not-index";
a["+3"] = "not-index";
console.log("len=" + a.length);
console.log("keys=" + Object.keys(a).join("|"));
console.log("json=" + JSON.stringify(a));

// The 2^32-2 boundary lives in claude-array-index-upper-boundary.ts — it is
// the one case that forces an engine to store sparsely, and keeping it apart
// stops it from hiding the rest of this file.

// A negative or fractional index never grows length.
const d: any = [1];
d[-1] = "neg";
d[1.5] = "frac";
console.log("dLen=" + d.length + " dKeys=" + Object.keys(d).join("|"));
console.log("dNeg=" + d[-1] + " dAt=" + d.at(-1));

// Number keys and their string forms are the same property.
const e: any = [];
e[2] = "x";
console.log("sameKey=" + (e["2"] === e[2]) + " has=" + ("2" in e) + " len=" + e.length);

// A large sparse index leaves holes, not elements.
const f: any = [];
f[5] = "five";
console.log("sparseLen=" + f.length + " in0=" + (0 in f) + " keys=" + Object.keys(f).join("|"));
console.log("sparseJoin=" + f.join(","));

// for-in reports index keys as strings, in ascending numeric order, before the
// non-index string keys.
const g: any = [];
g[3] = "d"; g[1] = "b"; g.zz = "z"; g[0] = "a"; g.aa = "y";
const order: string[] = [];
for (const k in g) order.push(k);
console.log("forIn=" + order.join(","));
console.log("ownKeys=" + Reflect.ownKeys(g).map(String).join(","));

// Symbol keys never participate.
const sym = Symbol("s");
const h: any = [1];
h[sym] = "sym";
console.log("symLen=" + h.length + " symKeys=" + Object.keys(h).join("|") + " symJson=" + JSON.stringify(h));
