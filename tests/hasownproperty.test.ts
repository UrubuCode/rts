import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR5) instance.hasOwnProperty(key) verifica own props sem
// seguir __proto__ chain.

const proto: number = collections.map_new();
collections.map_set(proto, "kind", 7);

const inst: number = Object.create(proto) as any;
collections.map_set(inst, "id", 42);

// Own
const ownId: boolean = (inst as any).hasOwnProperty("id");
print("own id=" + ownId);

// Inheritado — nao eh own
const ownKind: boolean = (inst as any).hasOwnProperty("kind");
print("own kind=" + ownKind);

// Inexistente em ambos
const ownMissing: boolean = (inst as any).hasOwnProperty("xyz");
print("own xyz=" + ownMissing);

// Em fn constructor: this.x = v cria own props
function Box(width: number, height: number): void {
  collections.map_set(this as any, "w", width);
  collections.map_set(this as any, "h", height);
}
Box.prototype.kind = 99 as any;

const b: number = new (Box as any)(3, 5);
const ownW: boolean = (b as any).hasOwnProperty("w");
const ownH: boolean = (b as any).hasOwnProperty("h");
const ownK: boolean = (b as any).hasOwnProperty("kind");
print("box.own w=" + ownW);
print("box.own h=" + ownH);
print("box.own kind=" + ownK);

describe("hasOwnProperty (#264 PR5)", () => {
  test("distingue own de inheritado em Object.create e new fn", () =>
    expect(__rtsCapturedOutput).toBe(
      "own id=true\n" +
      "own kind=false\n" +
      "own xyz=false\n" +
      "box.own w=true\n" +
      "box.own h=true\n" +
      "box.own kind=false\n"
    ));
});
