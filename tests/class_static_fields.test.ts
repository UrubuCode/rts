import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class Counter {
  static count: number = 0;
  static readonly MAX: number = 100;

  value: number;

  constructor(v: number) {
    this.value = v;
    Counter.count++;
  }

  static reset(): void {
    Counter.count = 0;
  }

  static getCount(): number {
    return Counter.count;
  }
}

// Static field access
print(`${Counter.MAX}`);
print(`${Counter.count}`);

new Counter(1);
new Counter(2);
new Counter(3);
print(`${Counter.getCount()}`);

Counter.reset();
print(`${Counter.count}`);

// Static fields in inheritance
class SpecialCounter extends Counter {
  static type: string = "special";

  constructor(v: number) {
    super(v);
  }
}

new SpecialCounter(10);
print(`${SpecialCounter.type}`);
print(`${Counter.getCount()}`);

describe("class_static_fields", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe("100\n0\n3\n0\nspecial\n1\n"));
});
