// ONE thing: Array.from's THREE input paths — an iterable, an array-like, and
// a value that is neither — and where the mapfn runs relative to each pull.
const order: string[] = [];

function* gen() { order.push("y1"); yield 1; order.push("y2"); yield 2; }
const a = Array.from(gen(), (v, i) => { order.push("m" + i); return v * 10; });
console.log("iterable=" + a.join(",") + " order=" + order.join(" "));

// An array-like is read by index 0..length-1, so holes become undefined.
const like: any = { length: 3, 0: "a", 2: "c" };
console.log("arrayLike=" + Array.from(like).map(String).join(","));
console.log("arrayLikeIn=" + Array.from(like).map((_v, i, s) => (i in s ? "y" : "n")).join(""));

// length is coerced with ToLength: negative and NaN clamp to 0, fractional truncates.
console.log("negLen=" + Array.from({ length: -1 } as any).length);
console.log("nanLen=" + Array.from({ length: NaN } as any).length);
console.log("fracLen=" + Array.from({ length: 2.9 } as any).length);
console.log("strLen=" + Array.from({ length: "2" } as any).length);
console.log("noLen=" + Array.from({} as any).length);

// A string goes through the ITERABLE path, so surrogate pairs stay whole.
console.log("string=" + Array.from("a\u{1F600}b").length);
console.log("stringLike=" + Array.from({ length: 3, 0: "x" } as any).length);

// The mapfn sees (value, index) only — never the source.
const seen: string[] = [];
Array.from([7, 8], function (...args: any[]) { seen.push(String(args.length)); return 0; });
console.log("mapArity=" + seen.join(","));

// Array.from is generic: called on a constructor, it uses it.
function Ctor(this: any, n?: number) { this.marker = "C"; this.length = n; }
const made: any = (Array.from as any).call(Ctor, [1, 2]);
console.log("ctor=" + made.marker + " len=" + made.length + " v=" + made[0] + made[1]);

// A non-callable mapfn is a TypeError, checked BEFORE the iterable is pulled.
const probe: any = { [Symbol.iterator]() { order.push("pulled"); return [][Symbol.iterator](); } };
try { (Array.from as any)(probe, 1); } catch (e: any) { console.log("badMap=" + e.constructor.name); }
console.log("pulledAfterBadMap=" + order.includes("pulled"));

// A Set and a Map go through the iterable path.
console.log("set=" + Array.from(new Set([1, 1, 2])).join(","));
console.log("map=" + Array.from(new Map([[1, "a"]])).map((p) => p[0] + ":" + p[1]).join(","));

// Array.of does not consult length at all.
console.log("of=" + Array.of(3).length + " ctor=" + new Array(3).length);
