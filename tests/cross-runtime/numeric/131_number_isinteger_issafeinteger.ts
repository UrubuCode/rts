// Cross-runtime: Number.isInteger and Number.isSafeInteger
console.log("isInt_5=" + Number.isInteger(5));
console.log("isInt_5.0=" + Number.isInteger(5.0));
console.log("isInt_5.5=" + Number.isInteger(5.5));
console.log("isInt_str=" + Number.isInteger("5"));
console.log("isInt_NaN=" + Number.isInteger(NaN));
console.log("isInt_Inf=" + Number.isInteger(Infinity));

const maxSafe = Number.MAX_SAFE_INTEGER;
const minSafe = Number.MIN_SAFE_INTEGER;
console.log("isSafe_100=" + Number.isSafeInteger(100));
console.log("isSafe_max=" + Number.isSafeInteger(maxSafe));
console.log("isSafe_max+1=" + Number.isSafeInteger(maxSafe + 1));
console.log("isSafe_min=" + Number.isSafeInteger(minSafe));
console.log("isSafe_5.5=" + Number.isSafeInteger(5.5));
