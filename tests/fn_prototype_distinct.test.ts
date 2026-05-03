import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR1) Cada user fn tem seu proprio prototype Map distinto.

function Animal(): void {}
function Plant(): void {}

const a: number = Animal.prototype as any;
const p: number = Plant.prototype as any;

print("distinct=" + (a !== p));
print("a_nonzero=" + (a !== 0));
print("p_nonzero=" + (p !== 0));

describe("prototype distinto por fn (#264 PR1)", () => {
  test("Animal.prototype != Plant.prototype, ambos non-zero", () =>
    expect(__rtsCapturedOutput).toBe(
      "distinct=true\n" +
      "a_nonzero=true\n" +
      "p_nonzero=true\n"
    ));
});
