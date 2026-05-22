// Cross-runtime: Date.now and valueOf (deterministic)
const d1 = new Date(2024, 0, 1);
const d2 = new Date(2024, 0, 1);

console.log("equal=" + (d1.valueOf() === d2.valueOf()));
console.log("valueOf=" + d1.valueOf());

const d3 = new Date(2024, 0, 2);
console.log("greater=" + (d3.valueOf() > d1.valueOf()));

// getTime is same as valueOf
console.log("getTime=" + d1.getTime());
console.log("same=" + (d1.getTime() === d1.valueOf()));

// Epoch
const epoch = new Date(0);
console.log("epoch=" + epoch.valueOf());
