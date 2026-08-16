// Cross-runtime: toFixed rounds and pads across sign and magnitude boundaries.
console.log([(1.25).toFixed(1), (-1.25).toFixed(1), (12).toFixed(3)].join("|"));
console.log([(0.0001).toFixed(3), (999.5).toFixed(0)].join("|"));

