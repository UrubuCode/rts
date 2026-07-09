// Cross-runtime: OrdinaryToPrimitive (valueOf/toString chain) for plain objects,
// Date numeric coercion (valueOf), and sparse-array holes vs Object.keys.

// A plain object with a numeric valueOf coerces via valueOf for number hints.
const n: any = { valueOf() { return 3; } };
console.log(n < 4);        // true  (valueOf → 3)
console.log(n - 1);        // 2
console.log(n * 2);        // 6

// A plain object with a string toString coerces via toString for string hints
// (property key coercion uses ToString).
const k: any = { toString() { return "hi"; } };
const bag: any = { hi: "found" };
console.log(bag[k]);       // "found"

// Date numeric coercion prefers valueOf (the time value) for +/-/< , but the
// string form for `+` with a string sibling.
const d: any = new Date(0);
console.log(+d);           // 0
console.log(d < new Date(5)); // true

// Sparse array via an object-valued index whose toString names a far slot: the
// skipped indices are HOLES, skipped by Object.keys (only 0,1,5 enumerate).
const far: any = { toString() { return "5"; } };
const arr: any[] = ["a", "b"];
arr[far] = "z";
console.log(arr.length);              // 6
console.log(Object.keys(arr).join(",")); // "0,1,5"

// filter(Boolean) drops every falsy element (Boolean as a callback).
console.log([0, "", null, undefined, NaN, false, 1, "x"].filter(Boolean).length); // 2
