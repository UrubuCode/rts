// Cross-runtime: Array callback arguments
const arr = [10, 20, 30];
const results: string[] = [];

arr.forEach((val, idx, array) => {
  results.push(val + ":" + idx + ":" + array.length);
});
console.log("forEach=" + results.join(","));

const mapped = arr.map((val, idx) => val + idx);
console.log("map=" + mapped.join(","));

const filtered = arr.filter((val, idx) => idx > 0);
console.log("filter=" + filtered.join(","));

const found = arr.find((val, idx) => idx === 1);
console.log("find=" + found);

const foundIdx = arr.findIndex((val) => val === 30);
console.log("findIndex=" + foundIdx);
