// Cross-runtime: Array.some short-circuits
let count = 0;
const arr = [1, 2, 3, 4, 5];
const result = arr.some(x => {
  count++;
  return x > 2;
});

console.log("result=" + result);
console.log("count=" + count);

// All false
let count2 = 0;
const result2 = arr.some(x => {
  count2++;
  return x > 10;
});

console.log("result2=" + result2);
console.log("count2=" + count2);
