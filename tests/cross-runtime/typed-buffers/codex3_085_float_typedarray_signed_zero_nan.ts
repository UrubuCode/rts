// Cross-runtime: floating typed arrays preserve signed zero and NaN classification.
const values = new Float64Array([0, -0, NaN, Infinity, -Infinity]);
console.log(Object.is(values[0], 0), Object.is(values[1], -0));
console.log(Number.isNaN(values[2]), values[3], values[4]);
const copy = new Float32Array([1 / 3]);
console.log(copy[0] === 1 / 3, copy[0].toPrecision(8));

