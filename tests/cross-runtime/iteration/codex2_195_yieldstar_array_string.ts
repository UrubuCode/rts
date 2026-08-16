// Cross-runtime: yield star delegates to built-in array and string iterables.
function* combined() {
  yield* [1, 2];
  yield* "A😀";
  yield 3;
}
console.log([...combined()].map(String).join("|"));

