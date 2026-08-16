// Cross-runtime: own integer keys precede strings regardless of insertion order.
const o: any = {};
o.z = 1; o[10] = "ten"; o[2] = "two"; o.a = 2; o[1] = "one";
console.log(Object.keys(o).join("|"));
console.log(Object.values(o).join("|"));

