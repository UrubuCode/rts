import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) constructor-function sem args: corpo executa, `this` populado. Via
// canônica JS `this.x`/`obj.x` (antes usava `collections.map_*(this)`).

function Box(): void {
  this.filled = 1;
}

const b: any = new (Box as any)();
const filled: number = b.filled;
print("filled=" + filled);

// Vazia (sem this no body): `new Empty()` aloca uma instância vazia.
function Empty(): void {}
const e: any = new (Empty as any)();
print("e_nonzero=" + (e !== 0));

describe("new UserFn() sem args (#264 PR3)", () => {
  test("ctor sem args, corpo opcional", () =>
    expect(__rtsCapturedOutput).toBe(
      "filled=1\n" +
      "e_nonzero=true\n"
    ));
});
