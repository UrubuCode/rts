// Cross-runtime: Number predicates do not coerce their arguments.
const xs: any[] = [NaN, "NaN", Infinity, "3", 3, null, undefined];
console.log(xs.map(Number.isNaN).join(","));
console.log(xs.map(Number.isFinite).join(","));
console.log(xs.map(Number.isInteger).join(","));

