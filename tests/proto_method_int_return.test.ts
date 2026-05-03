import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#proto-method) Method que retorna int continua formatando como int
// (TPL_COERCE_AUTO detecta valor que NAO eh handle Entry::String).

function Counter(): void {}
Counter.prototype.value = function(): number { return 42; };
Counter.prototype.times2 = function(): number { return 100; };

const c: number = new (Counter as any)();
print(`${(c as any).value()}`);
print(`${(c as any).times2()}`);

describe("proto method retorna int (#proto-method)", () => {
  test("ints nao sao confundidos com handles", () =>
    expect(__rtsCapturedOutput).toBe(
      "42\n" +
      "100\n"
    ));
});
