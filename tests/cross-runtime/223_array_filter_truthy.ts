// Cross-runtime: Array.filter with truthy values
const arr = [0, 1, false, 2, "", 3, null, 4, undefined, 5];
const truthy = arr.filter(x => x);
console.log("truthy=" + truthy.join(","));

const nums = [1, 2, 3, 4, 5];
const evens = nums.filter(x => x % 2 === 0);
console.log("evens=" + evens.join(","));

const odds = nums.filter(x => x % 2 !== 0);
console.log("odds=" + odds.join(","));

// Empty result
const none = nums.filter(x => x > 10);
console.log("none=" + none.join(","));
