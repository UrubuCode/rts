import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR5) Cadeia de 2 niveis: grandparent -> parent -> child.
// Lookup chain walk segue ate achar.

const grandparent: number = collections.map_new();
collections.map_set(grandparent, "level", 1);
collections.map_set(grandparent, "ancient", 99);

const parent: number = Object.create(grandparent) as any;
collections.map_set(parent, "level", 2);  // override
collections.map_set(parent, "middle", 7);

const child: number = Object.create(parent) as any;
collections.map_set(child, "level", 3);  // override again
collections.map_set(child, "young", 5);

// Lookups
const ll: number = (child as any).level;       // own
const mm: number = (child as any).middle;      // 1 nivel acima
const aa: number = (child as any).ancient;     // 2 niveis acima
const yy: number = (child as any).young;       // own
const xx: number = (child as any).inexistent;  // nao existe

print("level=" + ll);
print("middle=" + mm);
print("ancient=" + aa);
print("young=" + yy);
print("missing=" + xx);

// hasOwnProperty corretamente distingue
const hasL: boolean = (child as any).hasOwnProperty("level");
const hasM: boolean = (child as any).hasOwnProperty("middle");
const hasA: boolean = (child as any).hasOwnProperty("ancient");
print("ownLevel=" + hasL);
print("ownMiddle=" + hasM);
print("ownAncient=" + hasA);

describe("Object.create cadeia 2 niveis (#264 PR5)", () => {
  test("walk multi-nivel + hasOwnProperty", () =>
    expect(__rtsCapturedOutput).toBe(
      "level=3\n" +
      "middle=7\n" +
      "ancient=99\n" +
      "young=5\n" +
      "missing=0\n" +
      "ownLevel=true\n" +
      "ownMiddle=false\n" +
      "ownAncient=false\n"
    ));
});
