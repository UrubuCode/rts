// Cross-runtime: Math methods and NaN/Infinity propagation.
console.log("floor=" + Math.floor(3.9));
console.log("ceil=" + Math.ceil(3.1));
console.log("round=" + Math.round(3.5));
console.log("trunc=" + Math.trunc(-3.9));
console.log("abs=" + Math.abs(-12));
console.log("sqrt=" + Math.sqrt(81));
console.log("pow=" + Math.pow(2, 5));
console.log("log=" + Math.log(1));
console.log("exp=" + Math.exp(1).toFixed(3));
console.log("min=" + Math.min(4, -2, 9));
console.log("max=" + Math.max(4, -2, 9));
console.log("sign_n=" + Math.sign(-7));
console.log("sign_z=" + Math.sign(0));
console.log("hypot=" + Math.hypot(3, 4, 12));
console.log("nan_add=" + String(NaN + 1));
console.log("inf_mul=" + String(Infinity * 2));
console.log("zero_div_zero=" + String(0 / 0));
