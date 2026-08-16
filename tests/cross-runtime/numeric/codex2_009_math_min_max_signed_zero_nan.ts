// Cross-runtime: Math min/max preserve signed zero and propagate NaN.
console.log(Object.is(Math.min(0, -0), -0), Object.is(Math.max(0, -0), 0));
console.log(Number.isNaN(Math.min(1, NaN, 2)), Number.isNaN(Math.max(NaN, 3)));

