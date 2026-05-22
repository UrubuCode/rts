// Cross-runtime: Number.toExponential
const num = 12345;
console.log("exp=" + num.toExponential());
console.log("exp2=" + num.toExponential(2));
console.log("exp4=" + num.toExponential(4));

const small = 0.00012;
console.log("small=" + small.toExponential());
console.log("small2=" + small.toExponential(2));

const negative = -123.45;
console.log("neg=" + negative.toExponential(2));
