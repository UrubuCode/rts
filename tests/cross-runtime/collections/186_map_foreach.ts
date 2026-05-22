// Cross-runtime: Map.forEach
const map = new Map([["a", 1], ["b", 2], ["c", 3]]);
const results: string[] = [];

map.forEach((value, key, m) => {
  results.push(key + ":" + value);
});

console.log("forEach=" + results.join(","));

// Set.forEach
const set = new Set([1, 2, 3]);
const setResults: number[] = [];

set.forEach((value, value2, s) => {
  setResults.push(value);
  console.log("same=" + (value === value2));
});

console.log("set=" + setResults.join(","));
