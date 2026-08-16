// Cross-runtime: Object.is differs from strict equality for NaN and signed zero.
const pairs: any[][] = [[NaN, NaN], [0, -0], [-0, -0], [1, 1], ["1", 1]];
console.log(pairs.map(([a, b]) => Object.is(a, b)).join(","));
console.log(pairs.map(([a, b]) => a === b).join(","));

