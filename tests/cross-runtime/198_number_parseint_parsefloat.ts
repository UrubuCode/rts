// Cross-runtime: Number.parseInt and parseFloat
console.log("parseInt_123=" + Number.parseInt("123"));
console.log("parseInt_ff_16=" + Number.parseInt("ff", 16));
console.log("parseInt_10_2=" + Number.parseInt("10", 2));

console.log("parseFloat_3.14=" + Number.parseFloat("3.14"));
console.log("parseFloat_1e2=" + Number.parseFloat("1e2"));

// Same as global
console.log("same_int=" + (Number.parseInt === parseInt));
console.log("same_float=" + (Number.parseFloat === parseFloat));
