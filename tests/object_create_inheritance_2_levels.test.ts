import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) Cadeia de 2 níveis: grandparent -> parent -> child via Object.create.
// O lookup anda a cadeia até achar; `hasOwnProperty` distingue own de herdado.
// (Modelo novo: object literals + atribuição direta, sem `collections.map_*`.)

const grandparent: any = { level: 1, ancient: 99 };

const parent: any = Object.create(grandparent);
parent.level = 2; // override (own no parent)
parent.middle = 7;

const child: any = Object.create(parent);
child.level = 3; // override de novo (own no child)
child.young = 5;

print("level=" + child.level); // own
print("middle=" + child.middle); // 1 nível acima
print("ancient=" + child.ancient); // 2 níveis acima
print("young=" + child.young); // own
print("missing=" + child.inexistent); // não existe → undefined

print("ownLevel=" + child.hasOwnProperty("level"));
print("ownMiddle=" + child.hasOwnProperty("middle"));
print("ownAncient=" + child.hasOwnProperty("ancient"));

describe("Object.create cadeia 2 níveis (#264)", () => {
  test("walk multi-nível + hasOwnProperty", () =>
    expect(__rtsCapturedOutput).toBe(
      "level=3\n" +
      "middle=7\n" +
      "ancient=99\n" +
      "young=5\n" +
      "missing=undefined\n" +
      "ownLevel=true\n" +
      "ownMiddle=false\n" +
      "ownAncient=false\n"
    ));
});
