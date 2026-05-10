// Cross-runtime: Number.isFinite and Number.isNaN
console.log("isFinite_5=" + Number.isFinite(5));
console.log("isFinite_Inf=" + Number.isFinite(Infinity));
console.log("isFinite_NaN=" + Number.isFinite(NaN));
console.log("isFinite_str=" + Number.isFinite("5"));

console.log("isNaN_NaN=" + Number.isNaN(NaN));
console.log("isNaN_5=" + Number.isNaN(5));
console.log("isNaN_str=" + Number.isNaN("NaN"));
console.log("isNaN_undef=" + Number.isNaN(undefined));

// Compare with global versions
console.log("global_isNaN_str=" + isNaN("NaN"));
console.log("global_isFinite_str=" + isFinite("5"));
