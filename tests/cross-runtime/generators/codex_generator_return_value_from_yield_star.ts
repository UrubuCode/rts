// Cross-runtime: yield* expression receives delegated iterator return value.
function* inner() {
  yield 1;
  return 9;
}
function* outer() {
  const v = yield* inner();
  return v * 2;
}

const it = outer();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));
