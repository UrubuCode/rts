// Cross-runtime compatibility: modern Set operations.
const a = new Set([1, 2, 3]);
const b = new Set([3, 4]);
console.log("union=" + Array.from(a.union(b)).join(","));
console.log("inter=" + Array.from(a.intersection(b)).join(","));
console.log("diff=" + Array.from(a.difference(b)).join(","));
