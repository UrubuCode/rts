// Cross-runtime: signed zero survives arithmetic and reciprocal observation.
const values = [0 * -1, -0 + -0, 1 / -Infinity, Math.sign(-0)];
console.log(values.map((v) => Object.is(v, -0)).join(","));
console.log(values.map((v) => 1 / v).join(","));

