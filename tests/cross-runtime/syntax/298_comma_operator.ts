// Cross-runtime: comma operator evaluation and last value.
let x = 0;
const y = (x += 1, x += 2, x += 3);
console.log("x=" + x);
console.log("y=" + y);
