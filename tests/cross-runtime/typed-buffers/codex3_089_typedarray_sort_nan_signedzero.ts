// Cross-runtime: numeric typed-array sort places NaN last and negative zero before positive zero.
const values = new Float64Array([NaN, 3, -0, 0, -2, NaN, 1]);
values.sort();
console.log([...values].map((x) => Number.isNaN(x) ? "NaN" : Object.is(x, -0) ? "-0" : String(x)).join(","));

