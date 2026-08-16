// Cross-runtime: array spread operands are evaluated left-to-right.
const order: string[] = [];
function part(name: string, values: number[]) { order.push(name); return values; }
const out = [...part("a", [1, 2]), 3, ...part("b", [4, 5])];
console.log(out.join(","));
console.log(order.join(","));

