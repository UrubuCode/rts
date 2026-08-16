// Cross-runtime: rounding functions differ predictably for negative fractions.
const xs = [-2.7, -2.5, -2.1, 2.1, 2.5, 2.7];
console.log(xs.map(Math.floor).join(","));
console.log(xs.map(Math.ceil).join(","));
console.log(xs.map(Math.trunc).join(","));

