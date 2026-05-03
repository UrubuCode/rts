import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#proto-method) Method em fn.prototype que retorna string handle
// agora aparece corretamente em template literal via TPL_COERCE_AUTO.

function Box(): void {
  collections.map_set(this as any, "n", 7);
}
Box.prototype.greet = function(): string { return "hello"; };
Box.prototype.farewell = function(): string { return "bye"; };

const b: number = new (Box as any)();
print(`${(b as any).greet()}`);
print(`${(b as any).farewell()}`);

describe("proto method retorna string em template (#proto-method)", () => {
  test("INVOKE_AUTO + TPL_COERCE_AUTO mostra handle como string", () =>
    expect(__rtsCapturedOutput).toBe(
      "hello\n" +
      "bye\n"
    ));
});
