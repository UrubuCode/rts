// Cross-runtime: Math.sign and Math.trunc
console.log("sign_5=" + Math.sign(5));
console.log("sign_-5=" + Math.sign(-5));
console.log("sign_0=" + Math.sign(0));
console.log("sign_-0=" + Math.sign(-0));
console.log("sign_NaN=" + Math.sign(NaN));

console.log("trunc_5.9=" + Math.trunc(5.9));
console.log("trunc_-5.9=" + Math.trunc(-5.9));
console.log("trunc_0.1=" + Math.trunc(0.1));
console.log("trunc_-0.1=" + Math.trunc(-0.1));
console.log("trunc_NaN=" + Math.trunc(NaN));

// Compare with floor/ceil
console.log("floor_-5.9=" + Math.floor(-5.9));
console.log("ceil_-5.9=" + Math.ceil(-5.9));
