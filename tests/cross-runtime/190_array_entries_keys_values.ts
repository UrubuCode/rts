// Cross-runtime: Array.entries, keys, values
const arr = ["a", "b", "c"];

const entries = Array.from(arr.entries());
console.log("entries=" + entries.map(e => e.join(":")).join(","));

const keys = Array.from(arr.keys());
console.log("keys=" + keys.join(","));

const values = Array.from(arr.values());
console.log("values=" + values.join(","));

// Sparse array
const sparse = [1, , 3];
const sparseKeys = Array.from(sparse.keys());
console.log("sparse_keys=" + sparseKeys.join(","));
