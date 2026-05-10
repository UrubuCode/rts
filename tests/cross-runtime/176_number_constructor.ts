// Cross-runtime: Number constructor and constants
console.log("MAX_VALUE=" + (Number.MAX_VALUE > 1e308));
console.log("MIN_VALUE=" + (Number.MIN_VALUE > 0 && Number.MIN_VALUE < 1e-300));
console.log("MAX_SAFE_INTEGER=" + Number.MAX_SAFE_INTEGER);
console.log("MIN_SAFE_INTEGER=" + Number.MIN_SAFE_INTEGER);
console.log("POSITIVE_INFINITY=" + Number.POSITIVE_INFINITY);
console.log("NEGATIVE_INFINITY=" + Number.NEGATIVE_INFINITY);
console.log("NaN=" + Number.NaN);

console.log("isNaN_NaN=" + Number.isNaN(Number.NaN));
console.log("EPSILON=" + (Number.EPSILON > 0 && Number.EPSILON < 1e-15));
