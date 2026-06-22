import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic class: Box<T>.

class Box<T> {
  value: T;
  constructor(v: T) { this.value = v; }
  get(): T { return this.value; }
  set(v: T): void { this.value = v; }
}

const b = new Box<i64>(42);
print(`${b.get()}`);

b.set(99);
print(`${b.get()}`);

describe("fixture:generic_class", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n99\n");
  });
});
