// Cross-runtime: Array.with method
const arr = [1, 2, 3, 4, 5];

const modified = arr.with(2, 99);
console.log("modified=" + modified.join(","));
console.log("original=" + arr.join(","));

const negative = arr.with(-1, 88);
console.log("negative=" + negative.join(","));

const first = arr.with(0, 11);
console.log("first=" + first.join(","));
