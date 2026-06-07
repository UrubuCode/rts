// Cross-runtime: switch uses strict equality with unusual numeric values.
function classify(v: any): string {
  switch (v) {
    case 0: return "zero";
    case "0": return "str-zero";
    case false: return "false";
    case NaN: return "nan-case";
    default: return Object.is(v, NaN) ? "nan-default" : "other";
  }
}

console.log(classify(0));
console.log(classify(-0));
console.log(classify("0"));
console.log(classify(false));
console.log(classify(NaN));
