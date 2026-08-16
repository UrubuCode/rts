// Cross-runtime: concat preserves holes and creates its result through species.
class Result extends Array {}
class Source extends Array {
  static get [Symbol.species]() { return Result; }
}
const source = new Source();
source.length = 3;
source[1] = "x";
const out = source.concat(["y"]);
console.log(out instanceof Result, out.length);
console.log(Object.keys(out).join(","), JSON.stringify(out));

