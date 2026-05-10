// Cross-runtime: number formatting and edge values.
console.log("fixed=" + (12.3456).toFixed(2));
console.log("bin=" + (10).toString(2));
console.log("oct=" + (10).toString(8));
console.log("hex=" + (255).toString(16));
console.log("prec=" + (123.456).toPrecision(5));
console.log("nan=" + String(NaN));
console.log("inf=" + String(Infinity));
console.log("negzero=" + String(-0));
