// Cross-runtime: includes coerces and clamps its position argument.
const s = "bananana";
console.log([undefined, -2, 2.8, 99].map((p: any) => s.includes("ana", p)).join(","));
console.log(s.includes("", Infinity), s.includes("b", NaN));

