// Cross-runtime: Array.from on an ARRAY-LIKE ({length, 0:..}) that has NO
// Symbol.iterator. Focus: the length-based fallback path, not the iterator.

// 1) plain array-like
const al = { length: 3, 0: "a", 1: "b", 2: "c" };
console.log("basic=" + Array.from(al as any).join(","));

// 2) missing indices become undefined (NOT holes) => length preserved
const gappy = { length: 4, 0: "x", 2: "z" };
const g = Array.from(gappy as any);
console.log("gappyLen=" + g.length);
console.log("gappy=" + g.map((v) => String(v)).join(","));
console.log("gappyHasIdx1=" + (1 in g));

// 3) length beyond the provided indices
console.log("overLen=" + Array.from({ length: 2 } as any).map((v) => String(v)).join(","));

// 4) length: 0 => empty
console.log("zeroLen=" + Array.from({ length: 0, 0: "ignored" } as any).length);

// 5) NO length property at all => empty array
console.log("noLen=" + Array.from({ 0: "a", 1: "b" } as any).length);

// 6) length is coerced via ToLength: string "3" works
console.log("strLen=" + Array.from({ length: "3", 0: 1, 1: 2, 2: 3 } as any).join(","));

// 7) negative length clamps to 0
console.log("negLen=" + Array.from({ length: -5, 0: "a" } as any).length);

// 8) fractional length truncates
console.log("fracLen=" + Array.from({ length: 2.9, 0: "a", 1: "b", 2: "c" } as any).join(","));

// 9) NaN / undefined length => 0
console.log("nanLen=" + Array.from({ length: NaN, 0: "a" } as any).length);
console.log("undefLen=" + Array.from({ length: undefined, 0: "a" } as any).length);

// 10) mapFn receives (value, index) on the array-like path
const mapped = Array.from({ length: 3, 0: 10, 1: 20, 2: 30 } as any, (v: any, i: number) => v + i);
console.log("mapFn=" + mapped.join(","));

// 11) index keys are STRING props: "0" and 0 are the same key
const strKeys = { length: 2, "0": "s0", "1": "s1" };
console.log("strKeys=" + Array.from(strKeys as any).join(","));

// 12) inherited index via prototype chain is still read
const proto = { 1: "fromProto" };
const child: any = Object.create(proto);
child.length = 2;
child[0] = "own";
console.log("protoIdx=" + Array.from(child).join(","));

// 13) a getter index is invoked
let getterHits = 0;
const withGetter: any = { length: 2, 0: "plain" };
Object.defineProperty(withGetter, "1", {
  get() {
    getterHits++;
    return "got";
  },
  enumerable: true
});
console.log("getter=" + Array.from(withGetter).join(",") + "|hits=" + getterHits);

// 14) an array-like whose length is huge is NOT used here — verify a
//     Symbol.iterator, when present, WINS over length
const both: any = {
  length: 3,
  0: "L0",
  1: "L1",
  2: "L2",
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        return i <= 2 ? { value: "I" + i, done: false } : { value: undefined, done: true };
      }
    };
  }
};
console.log("iteratorWins=" + Array.from(both).join(","));
