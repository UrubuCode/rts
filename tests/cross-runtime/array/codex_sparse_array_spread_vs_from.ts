// Cross-runtime: spread and Array.from materialize sparse holes as undefined.
const a = Array(3);
a[1] = "x";
const spread = [...a];
const from = Array.from(a);
console.log(spread.map((v, i) => i + ":" + String(v)).join(","));
console.log(from.map((v, i) => i + ":" + String(v)).join(","));
console.log(Object.keys(spread).join(","));
console.log(Object.keys(a).join(","));
