// Cross-runtime: concat coerces arguments left-to-right.
const order: string[] = [];
const a = { toString() { order.push("a"); return "A"; } };
const b = { toString() { order.push("b"); return "B"; } };
console.log("x".concat(a as any, 2 as any, b as any));
console.log(order.join(","));

