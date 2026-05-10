// Cross-runtime: Object.is
console.log("is_5_5=" + Object.is(5, 5));
console.log("is_5_str5=" + Object.is(5, "5"));
console.log("is_NaN_NaN=" + Object.is(NaN, NaN));
console.log("eq_NaN_NaN=" + (NaN === NaN));

console.log("is_0_neg0=" + Object.is(0, -0));
console.log("eq_0_neg0=" + (0 === -0));

console.log("is_Inf_Inf=" + Object.is(Infinity, Infinity));
console.log("is_obj=" + Object.is({}, {}));

const obj = {};
console.log("is_same_obj=" + Object.is(obj, obj));
