// Cross-runtime: Number.toString without base
const num = 42;
console.log("default=" + num.toString());
console.log("explicit_10=" + num.toString(10));

const zero = 0;
console.log("zero=" + zero.toString());

const negative = -123;
console.log("negative=" + negative.toString());

const float = 3.14;
console.log("float=" + float.toString());

const large = 1000000;
console.log("large=" + large.toString());
