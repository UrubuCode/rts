// Cross-runtime: extracted class methods accept a replacement receiver via call.
class Adder {
  base: number;
  constructor(base: number) { this.base = base; }
  add(n: number) { return this.base + n; }
}
const method = new Adder(5).add;
console.log(method.call({ base: 20 }, 3));
console.log(method.call({ base: -2 }, 4));
