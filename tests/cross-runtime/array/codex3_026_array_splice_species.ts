// Cross-runtime: splice uses Array species for the removed-elements result.
class Removed extends Array {}
class Source extends Array {
  static get [Symbol.species]() { return Removed; }
}
const source = new Source(1, 2, 3, 4);
const removed = source.splice(1, 2, 9);
console.log(removed instanceof Removed, removed instanceof Source, removed.join(","));
console.log(source instanceof Source, source.join(","));

