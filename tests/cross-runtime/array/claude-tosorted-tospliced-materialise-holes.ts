// ONE thing: the ES2023 copying methods have no holes. Each reads the source
// with Get (not HasProperty), so a hole becomes an own undefined.
const src: any[] = [1, , 3, , 5];
console.log("srcIn=" + [0, 1, 2, 3, 4].map((i) => (i in src ? "y" : "n")).join(""));

const r = src.toReversed();
console.log("revIn=" + [0, 1, 2, 3, 4].map((i) => (i in r ? "y" : "n")).join(""));
console.log("rev=" + r.map((v) => String(v)).join(","));

const w = src.with(1, 99);
console.log("withIn=" + [0, 1, 2, 3, 4].map((i) => (i in w ? "y" : "n")).join(""));
console.log("with=" + w.map((v) => String(v)).join(","));

const sp = src.toSpliced(1, 2, "a", "b", "c");
console.log("splicedLen=" + sp.length);
console.log("spliced=" + sp.map((v) => String(v)).join(","));

console.log("srcAfter=" + src.length + " " + [0, 1, 2, 3, 4].map((i) => (i in src ? "y" : "n")).join(""));

console.log("withNeg=" + src.with(-1, "Z").map((v) => String(v)).join(","));
try { src.with(5, 0); } catch (e: any) { console.log("withOOB=" + e.constructor.name); }
try { src.with(-6, 0); } catch (e: any) { console.log("withNegOOB=" + e.constructor.name); }

console.log("toSplicedNoCount=" + src.toSpliced(2).map((v) => String(v)).join(","));
console.log("toSplicedNone=" + src.toSpliced().length);
