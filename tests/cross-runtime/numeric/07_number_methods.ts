// Cross-runtime: Number constants e methods.
console.log("MAX_SAFE=" + Number.MAX_SAFE_INTEGER);
console.log("MIN_SAFE=" + Number.MIN_SAFE_INTEGER);
console.log("isNaN(NaN)=" + Number.isNaN(NaN));
console.log("isNaN(0)=" + Number.isNaN(0));
console.log("isFinite(0)=" + Number.isFinite(0));
console.log("isFinite(Inf)=" + Number.isFinite(Infinity));
console.log("isInteger(0.5)=" + Number.isInteger(0.5));
console.log("isInteger(42)=" + Number.isInteger(42));
console.log("isSafeInteger(42)=" + Number.isSafeInteger(42));

console.log("parseInt(42)=" + parseInt("42"));
console.log("parseInt(42abc)=" + parseInt("42abc"));
console.log("parseInt(0xff)=" + parseInt("0xff"));
console.log("parseInt(10,2)=" + parseInt("10", 2));

console.log("(3.14).toFixed(1)=" + (3.14).toFixed(1));
console.log("(0.1+0.2).toFixed(1)=" + (0.1 + 0.2).toFixed(1));

console.log("1/0=" + (1 / 0));
console.log("-1/0=" + (-1 / 0));
console.log("0/0=" + (0 / 0));
