// Cross-runtime: global isFinite and isNaN
console.log("isFinite_5=" + isFinite(5));
console.log("isFinite_Inf=" + isFinite(Infinity));
console.log("isFinite_NaN=" + isFinite(NaN));
console.log("isFinite_str5=" + isFinite(Number("5")));

console.log("isNaN_NaN=" + isNaN(NaN));
console.log("isNaN_5=" + isNaN(5));
console.log("isNaN_str=" + isNaN(Number("abc")));

// Type coercion differences
console.log("global_str=" + isNaN("hello"));
console.log("number_str=" + Number.isNaN("hello"));
