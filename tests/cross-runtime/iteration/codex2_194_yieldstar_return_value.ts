// Cross-runtime: yield star forwards values and captures the delegate return.
function* inner() {
  yield 1;
  yield 2;
  return 7;
}
function* outer() {
  const result = yield* inner();
  yield result * 2;
}
console.log([...outer()].join(","));

