// Paridade JS/TS basica — rode com:
//   target/release/rts.exe run bench/parity_check.ts
//   bun bench/parity_check.ts
//   node --experimental-strip-types bench/parity_check.ts
// e compare as saidas linha a linha.

// ============ Array ============
console.log("--- Array ---");
const arr = [1, 2, 3, 4, 5, 2, 3];
console.log("indexOf(3):", arr.indexOf(3));
console.log("indexOf(3, 3):", arr.indexOf(3, 3));
console.log("lastIndexOf(2):", arr.lastIndexOf(2));
console.log("includes(4):", arr.includes(4));
console.log("includes(1, 2):", arr.includes(1, 2));
console.log("reverse:", [1, 2, 3].reverse().join(","));
console.log("slice(1,4):", [1, 2, 3, 4, 5].slice(1, 4).join(","));
console.log("concat:", [1, 2].concat([3, 4]).join(","));
console.log("fill(7):", [0, 0, 0].fill(7).join(","));
console.log("flat:", [[1, 2], [3, 4]].flat().join(","));
console.log("findLast<5:", [1, 5, 2, 8, 3].findLast((x: number) => x < 5));
console.log("findLastIndex<5:", [1, 5, 2, 8, 3].findLastIndex((x: number) => x < 5));
console.log("reduceRight:", [1, 2, 3, 4].reduceRight((a: number, b: number) => a - b, 0));
console.log("toSorted:", [3, 1, 2].toSorted().join(","));
console.log("toReversed:", [1, 2, 3].toReversed().join(","));
console.log("with(1,99):", [1, 2, 3].with(1, 99).join(","));
console.log("Array.from(len=3).length:", Array.from({ length: 3 }).length);

const stk = [1, 2, 3];
console.log("shift:", stk.shift());
console.log("after shift:", stk.join(","));
stk.unshift(0);
console.log("unshift:", stk.join(","));

const strs = ["banana", "apple", "cherry"];
strs.sort();
console.log("sort strings:", strs.join(","));

// ============ Object ============
console.log("--- Object ---");
const o = { a: 1, b: 2, c: 3 };
console.log("keys:", Object.keys(o).join(","));
console.log("values:", Object.values(o).join(","));
console.log("entries.length:", Object.entries(o).length);
console.log("assign:", JSON.stringify(Object.assign({}, { x: 1 }, { y: 2 })));
console.log("isFrozen:", Object.isFrozen(Object.freeze({ a: 1 })));
const fe = Object.fromEntries([["k1", 10], ["k2", 20]]);
console.log("fromEntries.k1+k2:", fe.k1 + fe.k2);

// ============ Math ============
console.log("--- Math ---");
console.log("sign(-5):", Math.sign(-5));
console.log("sign(7):", Math.sign(7));
console.log("hypot(3,4):", Math.hypot(3, 4));
console.log("imul(3,4):", Math.imul(3, 4));
console.log("clz32(1):", Math.clz32(1));
console.log("SQRT2:", Math.SQRT2.toFixed(4));
console.log("LN2:", Math.LN2.toFixed(4));
console.log("sinh(1):", Math.sinh(1).toFixed(4));

// ============ String ============
console.log("--- String ---");
console.log("split limit:", "a,b,c,d".split(",", 2).join("|"));
console.log("startsWith off:", "hello world".startsWith("world", 6));
console.log("endsWith off:", "hello world".endsWith("hello", 5));

// ============ Symbol ============
console.log("--- Symbol ---");
const s1 = Symbol("desc");
console.log("description:", s1.description);
const s2 = Symbol.for("shared");
const s3 = Symbol.for("shared");
console.log("for === for:", s2 === s3);
console.log("keyFor:", Symbol.keyFor(s2));

// ============ URL ============
console.log("--- URL ---");
const u = new URL("https://example.com/path?x=1&y=2#frag");
console.log("host:", u.host);
console.log("pathname:", u.pathname);
console.log("search:", u.search);
console.log("hash:", u.hash);

const sp = new URLSearchParams("a=1&b=2");
console.log("sp.get(a):", sp.get("a"));
console.log("sp.has(b):", sp.has("b"));
sp.set("a", "9");
console.log("sp.set(a,9):", sp.get("a"));

// ============ Date ============
console.log("--- Date ---");
const d = new Date(0);
console.log("toISOString:", d.toISOString());
console.log("toUTCString:", d.toUTCString());
const d2 = new Date(0);
d2.setFullYear(2025);
console.log("setFullYear getFullYear:", d2.getFullYear());

// ============ Boolean ============
console.log("--- Boolean ---");
console.log("Boolean(NaN):", Boolean(NaN));
console.log("Boolean(''):", Boolean(""));
console.log("Boolean(42):", Boolean(42));
console.log("(true).toString():", (true).toString());

// ============ parseInt ============
console.log("--- parseInt ---");
console.log("parseInt('ff',16):", parseInt("ff", 16));
console.log("parseInt('0x10'):", parseInt("0x10"));
console.log("parseInt('1010',2):", parseInt("1010", 2));

// ============ URI encoding ============
console.log("--- URI ---");
console.log("encodeURIComponent:", encodeURIComponent("a b&c"));
console.log("decodeURIComponent:", decodeURIComponent("a%20b%26c"));

// ============ Destructuring ============
console.log("--- Destructuring ---");
const [da, db, ...drest] = [1, 2, 3, 4, 5];
console.log("[a,b,...r]:", da, db, drest.join("/"));
const { x: dx, y: dy = 99 } = { x: 10 };
console.log("{x,y=99}:", dx, dy);
const [[na, nb]] = [[7, 8]];
console.log("[[a,b]]:", na, nb);

// ============ WeakMap ============
console.log("--- WeakMap ---");
const wm = new WeakMap();
const key = {};
wm.set(key, 42);
console.log("wm.get:", wm.get(key));
console.log("wm.has:", wm.has(key));

console.log("--- done ---");
