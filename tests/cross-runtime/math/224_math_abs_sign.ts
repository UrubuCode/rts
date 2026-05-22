// Cross-runtime: Math.abs
console.log("abs_5=" + Math.abs(5));
console.log("abs_-5=" + Math.abs(-5));
console.log("abs_0=" + Math.abs(0));
console.log("abs_-0=" + Math.abs(-0));
console.log("abs_inf=" + Math.abs(-Infinity));

console.log("abs_3.14=" + Math.abs(-3.14));
console.log("abs_str=" + Math.abs(Number("-10")));

// Compare with sign
console.log("sign_abs=" + Math.sign(Math.abs(-5)));
