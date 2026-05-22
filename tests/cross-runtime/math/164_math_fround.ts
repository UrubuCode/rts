// Cross-runtime: Math.fround (float32)
console.log("fround_1.5=" + Math.fround(1.5));
console.log("fround_1.337=" + Math.fround(1.337));
console.log("fround_0=" + Math.fround(0));
console.log("fround_NaN=" + Math.fround(NaN));
console.log("fround_Inf=" + Math.fround(Infinity));

// Precision loss
const precise = 1.0000001;
console.log("precise=" + (Math.fround(precise) === 1));
