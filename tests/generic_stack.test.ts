import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic class Stack<T> com push/pop.

class Stack<T> {
  items: T[] = [];

  push(v: T): void {
    const arr = this.items;
    arr.push(v);
  }

  count(): i64 {
    let n: i64 = 0;
    const arr = this.items;
    for (const _v of arr) n = n + 1;
    return n;
  }
}

const s = new Stack<i64>();
s.push(10);
s.push(20);
s.push(30);

const h = gc.string_from_i64(s.count());
print(h); gc.string_free(h);

describe("fixture:generic_stack", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3\n");
  });
});
