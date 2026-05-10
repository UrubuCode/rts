// Cross-runtime: Number.toFixed and toPrecision
const num = 123.456;
console.log("fixed0=" + num.toFixed(0));
console.log("fixed1=" + num.toFixed(1));
console.log("fixed2=" + num.toFixed(2));
console.log("fixed5=" + num.toFixed(5));

console.log("prec1=" + num.toPrecision(1));
console.log("prec3=" + num.toPrecision(3));
console.log("prec5=" + num.toPrecision(5));

const small = 0.0001;
console.log("small_fixed=" + small.toFixed(5));
console.log("small_prec=" + small.toPrecision(2));

const large = 12345;
console.log("large_fixed=" + large.toFixed(2));
