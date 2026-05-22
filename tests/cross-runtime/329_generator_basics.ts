// Cross-runtime: basic generator functions.
function* counter(start: number, end: number) {
  for (let i = start; i <= end; i++) yield i;
}

const gen = counter(1, 5);
for (const v of gen) console.log(v);

function* fibonacci() {
  let a = 0, b = 1;
  while (true) {
    yield a;
    [a, b] = [b, a + b];
  }
}

const fib = fibonacci();
const first8: number[] = [];
for (let i = 0; i < 8; i++) first8.push(fib.next().value as number);
console.log(first8.join(","));

// generator with return
function* withReturn() {
  yield 1;
  yield 2;
  return 99;
  yield 3;  // unreachable
}
const g = withReturn();
console.log(g.next().value);
console.log(g.next().value);
console.log(g.next().value);   // 99 (return value)
console.log(g.next().done);    // true
