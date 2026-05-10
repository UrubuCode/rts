// Cross-runtime: Array.reduce edge cases
const arr = [1, 2, 3, 4];
const sum = arr.reduce((acc, val) => acc + val, 0);
console.log("sum=" + sum);

const product = arr.reduce((acc, val) => acc * val, 1);
console.log("product=" + product);

const max = arr.reduce((acc, val) => Math.max(acc, val));
console.log("max=" + max);

// Single element
const single = [5].reduce((acc, val) => acc + val);
console.log("single=" + single);

// With initial value
const withInit = [5].reduce((acc, val) => acc + val, 10);
console.log("withInit=" + withInit);

// Building object
const pairs = [["a", 1], ["b", 2], ["c", 3]];
const obj = pairs.reduce((acc, [k, v]) => ({ ...acc, [k]: v }), {});
console.log("obj=" + JSON.stringify(obj));
