// Cross-runtime: Number.toString with different bases
const num = 255;
console.log("base10=" + num.toString(10));
console.log("base2=" + num.toString(2));
console.log("base8=" + num.toString(8));
console.log("base16=" + num.toString(16));

const small = 10;
console.log("small_2=" + small.toString(2));
console.log("small_16=" + small.toString(16));

const zero = 0;
console.log("zero=" + zero.toString(2));

const negative = -15;
console.log("neg_2=" + negative.toString(2));
console.log("neg_16=" + negative.toString(16));
