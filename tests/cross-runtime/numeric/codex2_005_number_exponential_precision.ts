// Cross-runtime: exponential formatting uses the requested fractional precision.
console.log([(1234).toExponential(2), (0.01234).toExponential(3)].join("|"));
console.log([(1).toExponential(0), (-42).toExponential(1)].join("|"));

