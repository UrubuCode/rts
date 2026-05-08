import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Basic generator
function* counter(start: number, end: number) {
  for (let i = start; i <= end; i++) {
    yield i;
  }
}

const gen = counter(1, 5);
for (const v of gen) {
  print(`${v}`);
}

// Generator with iterator protocol
function* fibonacci() {
  let a = 0, b = 1;
  while (true) {
    yield a;
    const tmp = a + b;
    a = b;
    b = tmp;
  }
}

const fib = fibonacci();
for (let i = 0; i < 8; i++) {
  print(`${fib.next().value}`);
}

// yield* delegation
function* inner() {
  yield 10;
  yield 20;
}
function* outer() {
  yield 1;
  yield* inner();
  yield 2;
}

for (const v of outer()) {
  print(`${v}`);
}

describe("generator_basic", () => {
  test("counter", () => expect(__rtsCapturedOutput).toBe(
    "1\n2\n3\n4\n5\n0\n1\n1\n2\n3\n5\n8\n13\n1\n10\n20\n2\n"
  ));
});
