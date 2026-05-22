// Cross-runtime: Array.reduceRight
const arr = [1, 2, 3, 4];
const result = arr.reduceRight((acc, val) => acc + val, 0);
console.log("sum=" + result);

const concat = arr.reduceRight((acc, val) => acc + "," + val);
console.log("concat=" + concat);

const nested = [[1, 2], [3, 4], [5, 6]];
const flat = nested.reduceRight((acc, val) => acc.concat(val), []);
console.log("flat=" + flat.join(","));

// Building number from digits
const digits = [1, 2, 3];
const num = digits.reduceRight((acc, val) => acc * 10 + val, 0);
console.log("num=" + num);
