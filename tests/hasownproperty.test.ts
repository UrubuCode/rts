import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) instance.hasOwnProperty(key) verifica own props SEM seguir a cadeia de
// __proto__. (Modelo novo: Object.create + atribuição direta own, sem
// `collections.map_*` nem function-constructor com `.prototype`.)

const proto: any = { kind: 7 };

const inst: any = Object.create(proto);
inst.id = 42; // own

print("own id=" + inst.hasOwnProperty("id")); // own → true
print("own kind=" + inst.hasOwnProperty("kind")); // herdado → false
print("own xyz=" + inst.hasOwnProperty("xyz")); // ausente → false

// Outra instância com proto + próprias: own props vêm da atribuição direta,
// a herdada (do proto) NÃO é own.
const shared: any = { kind: 99 };
const box: any = Object.create(shared);
box.w = 3;
box.h = 5;
print("box.own w=" + box.hasOwnProperty("w"));
print("box.own h=" + box.hasOwnProperty("h"));
print("box.own kind=" + box.hasOwnProperty("kind"));

describe("hasOwnProperty (#264)", () => {
  test("distingue own de herdado em Object.create", () =>
    expect(__rtsCapturedOutput).toBe(
      "own id=true\n" +
      "own kind=false\n" +
      "own xyz=false\n" +
      "box.own w=true\n" +
      "box.own h=true\n" +
      "box.own kind=false\n"
    ));
});
