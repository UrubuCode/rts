// Cross-runtime: Math.pow
console.log("2_3=" + Math.pow(2, 3));
console.log("10_2=" + Math.pow(10, 2));
console.log("5_0=" + Math.pow(5, 0));
console.log("2_-1=" + Math.pow(2, -1));
console.log("4_0.5=" + Math.pow(4, 0.5));

console.log("neg_2=" + Math.pow(-2, 2));
console.log("neg_3=" + Math.pow(-2, 3));

// Compare with ** operator
console.log("operator=" + (2 ** 3));
console.log("same=" + (Math.pow(2, 3) === 2 ** 3));
