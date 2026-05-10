// Cross-runtime: Array.reduce without initial value
const arr = [1, 2, 3, 4];
const sum = arr.reduce((acc, val) => acc + val);
console.log("sum=" + sum);

const product = arr.reduce((acc, val) => acc * val);
console.log("product=" + product);

const single = [42].reduce((acc, val) => acc + val);
console.log("single=" + single);

// String concatenation
const words = ["hello", "world"];
const sentence = words.reduce((acc, val) => acc + " " + val);
console.log("sentence=" + sentence);
