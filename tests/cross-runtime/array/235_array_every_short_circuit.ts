// Cross-runtime: Array.every short-circuits
let count = 0;
const arr = [1, 2, 3, 4, 5];
const result = arr.every(x => {
  count++;
  return x < 3;
});

console.log("result=" + result);
console.log("count=" + count);

// All true
let count2 = 0;
const result2 = arr.every(x => {
  count2++;
  return x < 10;
});

console.log("result2=" + result2);
console.log("count2=" + count2);
