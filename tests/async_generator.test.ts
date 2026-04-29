import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Async generator + for-await-of
async function* asyncRange(start: number, end: number) {
  for (let i = start; i <= end; i++) {
    yield i;
  }
}

for await (const v of asyncRange(1, 5)) {
  print(`${v}`);
}

// Async generator com lógica
async function* processItems(items: number[]) {
  for (const item of items) {
    if (item % 2 === 0) {
      yield item * 10;
    }
  }
}

const results: number[] = [];
for await (const v of processItems([1, 2, 3, 4, 5, 6])) {
  results.push(v);
}
print(results.join(","));

// yield* em async generator
async function* inner() {
  yield "a";
  yield "b";
}
async function* outer() {
  yield "start";
  yield* inner();
  yield "end";
}

const parts: string[] = [];
for await (const v of outer()) {
  parts.push(v);
}
print(parts.join(","));

describe("async_generator", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "1\n2\n3\n4\n5\n20,40,60\nstart,a,b,end\n"
  ));
});
