// Cross-runtime: Array species controls the constructor used by derived array methods.
class PlainResultArray extends Array {
  static get [Symbol.species]() { return Array; }
}
const source = new PlainResultArray(1, 2, 3);
const mapped = source.map((x) => x * 2);
const sliced = source.slice(1);
console.log(mapped instanceof PlainResultArray, mapped instanceof Array, mapped.join(","));
console.log(sliced instanceof PlainResultArray, sliced.constructor === Array);

