// Cross-runtime: Array.sort with equal elements
const arr = [
  { id: 1, val: 5 },
  { id: 2, val: 3 },
  { id: 3, val: 5 },
  { id: 4, val: 3 }
];

arr.sort((a, b) => a.val - b.val);
const ids = arr.map(x => x.id).join(",");
console.log("sorted=" + ids);

// Simple numbers
const nums = [3, 1, 4, 1, 5, 9, 2, 6];
nums.sort((a, b) => a - b);
console.log("nums=" + nums.join(","));
