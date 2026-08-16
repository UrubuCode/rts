// Cross-runtime: flat coerces depth and creates a species-selected dense result.
class Result extends Array {}
class Nested extends Array {
  static get [Symbol.species]() { return Result; }
}
const source: any = new Nested();
source.push(1, [2, [3, [4]]]);
source[4] = 5;
source.length = 6;
const out = source.flat("2.9" as any);
console.log(out instanceof Result, out.join(","), out.length);
console.log(Object.keys(out).join(","));

