// Cross-runtime: apply consumes an array-like argument object.
function join(this: any, a: any, b: any, c: any) {
  return this.prefix + [a, b, c].join("-");
}
const args = { 0: "x", 1: "y", 2: "z", length: 3 };
console.log(join.apply({ prefix: ">" }, args as any));

