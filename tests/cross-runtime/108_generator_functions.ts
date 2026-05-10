// Cross-runtime: generator functions and iteration
function* simpleGen() {
  yield 1;
  yield 2;
  yield 3;
}

const gen = simpleGen();
console.log("next1=" + JSON.stringify(gen.next()));
console.log("next2=" + JSON.stringify(gen.next()));
console.log("next3=" + JSON.stringify(gen.next()));
console.log("next4=" + JSON.stringify(gen.next()));

function* genWithReturn() {
  yield "a";
  yield "b";
  return "done";
}

const gen2 = genWithReturn();
const results: string[] = [];
for (const val of gen2) {
  results.push(val);
}
console.log("forof=" + results.join(","));

function* fibonacci() {
  let a = 0, b = 1;
  for (let i = 0; i < 5; i++) {
    yield a;
    [a, b] = [b, a + b];
  }
}

console.log("fib=" + Array.from(fibonacci()).join(","));
