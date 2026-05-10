// Cross-runtime compatibility: number formatting edge cases.
console.log("fixed_nan=" + (NaN).toFixed(2));
console.log("fixed_inf=" + (Infinity).toFixed(2));
console.log("prec_small=" + (0.0001234).toPrecision(3));
console.log("prec_big=" + (123456.789).toPrecision(4));
console.log("exp_pos=" + (12.34).toExponential(2));
console.log("exp_neg=" + (-12.34).toExponential(1));
console.log("radix36=" + (123456).toString(36));
console.log("neg_zero=" + Object.is(-0, 0) + ":" + String(-0));
