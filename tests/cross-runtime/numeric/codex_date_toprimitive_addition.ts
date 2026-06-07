// Cross-runtime: Date prefers string hint for addition but number for unary plus.
const d: any = new Date(0);
const sum = d + 1;
console.log(typeof sum);
console.log(sum.endsWith("1"));
console.log(+d);
console.log(d < new Date(1));
