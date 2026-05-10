// Cross-runtime: Set operation family.
const a = new Set([1, 2, 3]);
const b = new Set([3, 4]);
console.log("union=" + Array.from(a.union(b)).join(","));
console.log("sym=" + Array.from(a.symmetricDifference(b)).join(","));
console.log("subset=" + new Set([1, 2]).isSubsetOf(a));
console.log("sup=" + a.isSupersetOf(new Set([2])));
console.log("dis=" + a.isDisjointFrom(new Set([9])));
