// Cross-runtime: Math.min and max with no args
console.log("min_empty=" + Math.min());
console.log("max_empty=" + Math.max());

console.log("min_1=" + Math.min(1));
console.log("max_1=" + Math.max(1));

console.log("min_neg=" + Math.min(-5, -10, -1));
console.log("max_neg=" + Math.max(-5, -10, -1));

console.log("min_mixed=" + Math.min(5, -3, 0, 10));
console.log("max_mixed=" + Math.max(5, -3, 0, 10));

// With Infinity
console.log("min_inf=" + Math.min(1, Infinity));
console.log("max_inf=" + Math.max(1, -Infinity));
