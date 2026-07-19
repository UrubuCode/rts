function dbl(x: number): number { return x * 2; }
const arr = [1, 2, 3, 4, 5];
const out = arr.map(dbl);
let sum = 0;
for (const v of out) { sum = sum + v; }
console.log("sum:" + sum);
