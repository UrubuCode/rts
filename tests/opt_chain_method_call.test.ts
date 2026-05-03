import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #432: obj?.method() com obj null retorna undefined em template,
// nao crash nem 0.

class Greeter {
  greet(): number { return 7; }
}

const g1: Greeter | null = null;
print(`a=${g1?.greet()}`);

const g2: Greeter | null = new Greeter();
print(`b=${g2?.greet()}`);

describe("obj?.method() com null receiver (#432)", () => {
  test("null -> undefined; non-null -> retorno real", () => {
    expect(__rtsCapturedOutput).toBe(
      "a=undefined\n" +
      "b=7\n"
    );
  });
});
