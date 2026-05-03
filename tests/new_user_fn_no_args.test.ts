import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) constructor sem args: corpo executa, `this` populado.

function Box(): void {
  collections.map_set(this as any, "filled", 1);
}

const b: number = new (Box as any)();
const filled: number = collections.map_get(b, "filled");
print("filled=" + filled);

// Vazia (sem this no body): `this` ainda eh alocado mas Map vazio
function Empty(): void {}
const e: number = new (Empty as any)();
print("e_nonzero=" + (e !== 0));

describe("new UserFn() sem args (#264 PR3)", () => {
  test("ctor sem args, corpo opcional", () =>
    expect(__rtsCapturedOutput).toBe(
      "filled=1\n" +
      "e_nonzero=true\n"
    ));
});
