// Cross-runtime: bind fixes a receiver and prepends partial arguments.
function sum(this: any, a: number, b: number, c: number) {
  return this.base + a + b + c;
}
const bound = sum.bind({ base: 10 }, 1, 2);
console.log(bound(3), bound(-3));
console.log(bound.length);

