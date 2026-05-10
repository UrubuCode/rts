// Cross-runtime: parseInt and parseFloat
console.log("int_123=" + parseInt("123"));
console.log("int_123.45=" + parseInt("123.45"));
console.log("int_0x10=" + parseInt("0x10"));
console.log("int_10_base2=" + parseInt("10", 2));
console.log("int_10_base8=" + parseInt("10", 8));
console.log("int_10_base16=" + parseInt("10", 16));
console.log("int_ff_base16=" + parseInt("ff", 16));

console.log("float_123.45=" + parseFloat("123.45"));
console.log("float_0.5=" + parseFloat("0.5"));
console.log("float_.5=" + parseFloat(".5"));
console.log("float_3.14e2=" + parseFloat("3.14e2"));

console.log("int_abc=" + parseInt("abc"));
console.log("float_abc=" + parseFloat("abc"));
console.log("int_12abc=" + parseInt("12abc"));
