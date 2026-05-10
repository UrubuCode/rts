// Cross-runtime: Array.concat
const arr1 = [1, 2];
const arr2 = [3, 4];
console.log("basic=" + arr1.concat(arr2).join(","));
console.log("original=" + arr1.join(","));

const multi = [1].concat([2], [3], [4]);
console.log("multi=" + multi.join(","));

const withValues = [1, 2].concat(3, 4);
console.log("values=" + withValues.join(","));

const nested = [1].concat([[2, 3]]);
console.log("nested=" + nested.join(","));

const empty = [].concat([1, 2]);
console.log("empty=" + empty.join(","));
