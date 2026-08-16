// ONE thing: four ways to copy a collection use three DIFFERENT protocols —
// spread and Array.from(iterable) pull the iterator, Array.from(array-like)
// reads by index, and concat/slice read by index while preserving holes.
const pulls: string[] = [];
const custom: any = {
  length: 3,
  0: "i0", 1: "i1", 2: "i2",
  [Symbol.iterator]() {
    let i = 0;
    return { next: () => { pulls.push("pull" + i); return i < 2 ? { value: "it" + i++, done: false } : { value: undefined, done: true }; } };
  },
};
console.log("spread=" + [...custom].join(",") + " pulls=" + pulls.join(" "));
pulls.length = 0;
console.log("from=" + Array.from(custom).join(",") + " pulls=" + pulls.join(" "));
pulls.length = 0;
console.log("concat=" + ([] as any).concat(custom).length + " pulls=" + pulls.join(" "));
console.log("sliceCall=" + Array.prototype.slice.call(custom).join(",") + " pulls=" + pulls.join(" "));

// An ARRAY with a patched Symbol.iterator: spread and from follow it, the
// index-based methods ignore it entirely.
const patched: any = [1, 2, 3];
patched[Symbol.iterator] = function* () { yield "A"; yield "B"; };
console.log("patchedSpread=" + [...patched].join(","));
console.log("patchedFrom=" + Array.from(patched).join(","));
console.log("patchedSlice=" + patched.slice().join(","));
console.log("patchedConcat=" + ([] as any).concat(patched).join(","));
console.log("patchedJoin=" + patched.join(","));
console.log("patchedForOf=" + (() => { const o: string[] = []; for (const v of patched) o.push(String(v)); return o.join(","); })());

// Holes: the iterator materialises them as undefined, slice/concat keep them.
const h: any[] = [1, , 3];
const sp = [...h], fr = Array.from(h), sl = h.slice(), cc = ([] as any).concat(h);
const inMap = (x: any[]) => [0, 1, 2].map((i) => (i in x ? "y" : "n")).join("");
console.log("holes spread=" + inMap(sp) + " from=" + inMap(fr) + " slice=" + inMap(sl) + " concat=" + inMap(cc));

// A string: spread and from split by CODE POINT, slice-call by code unit.
const s = "a\u{1F600}";
console.log("strSpread=" + [...s].length + " strFrom=" + Array.from(s).length + " strSlice=" + Array.prototype.slice.call(s).length);

// A Set and a Map only have the iterator protocol; index reads find nothing.
const set = new Set([1, 2]);
console.log("setSpread=" + [...set].join(",") + " setSliceLen=" + Array.prototype.slice.call(set as any).length);

// Spread into a CALL and into an object literal use different protocols:
// the call spreads the iterator, the object literal copies own enumerable keys.
function count(...args: any[]) { return args.length + ":" + args.join("|"); }
console.log("callSpread=" + count(...custom));
console.log("objSpread=" + JSON.stringify({ ...custom }));
console.log("objSpreadArray=" + JSON.stringify({ ...[1, , 3] }));
console.log("objSpreadString=" + JSON.stringify({ ..."ab" }));

// A non-iterable in a spread position is a TypeError; in an object literal it
// is silently ignored.
try { console.log([...({ a: 1 } as any)].length); } catch (e: any) { console.log("spreadPlain=" + e.constructor.name); }
console.log("objSpreadNull=" + JSON.stringify({ ...(null as any), k: 1 }));
console.log("fromPlain=" + Array.from({ a: 1 } as any).length);

// A Symbol.iterator that is not callable is a TypeError for spread but leaves
// Array.from to fall back to the array-like path only when it is undefined.
const bad: any = { length: 1, 0: "z", [Symbol.iterator]: 42 };
try { [...bad]; } catch (e: any) { console.log("badIterSpread=" + e.constructor.name); }
try { Array.from(bad); } catch (e: any) { console.log("badIterFrom=" + e.constructor.name); }
const undefIter: any = { length: 1, 0: "z", [Symbol.iterator]: undefined };
console.log("undefIterFrom=" + Array.from(undefIter).join(","));
