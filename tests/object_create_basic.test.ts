import { describe, test, expect } from "rts:test";
import { collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR5) Object.create(proto) aloca novo Map com __proto__ apontando
// pro proto. Lookup chain segue para o proto quando key nao eh own.

const proto: number = collections.map_new();
collections.map_set(proto, "kind", 7);
collections.map_set(proto, "color", 9);

const inst: number = Object.create(proto) as any;
print("nonzero=" + (inst !== 0));

// Lookups inheritados
const k: number = (inst as any).kind;
const c: number = (inst as any).color;
print("kind=" + k);
print("color=" + c);

// Adiciona own e sobrescreve um proto field
collections.map_set(inst, "id", 99);
collections.map_set(inst, "kind", 1);  // sobrescreve

const id: number = (inst as any).id;
const kAfter: number = (inst as any).kind;
const colorStill: number = (inst as any).color;
print("id=" + id);
print("kind own=" + kAfter);
print("color still inh=" + colorStill);

// Object.create(0) — equivalent a Object.create(null), sem chain
const noProto: number = Object.create(0 as any) as any;
const lookupMissing: number = (noProto as any).kind;
print("noProto.kind=" + lookupMissing);

describe("Object.create + chain (#264 PR5)", () => {
  test("aloca, herda do proto, own sobrescreve, sem proto = 0 chain", () =>
    expect(__rtsCapturedOutput).toBe(
      "nonzero=true\n" +
      "kind=7\n" +
      "color=9\n" +
      "id=99\n" +
      "kind own=1\n" +
      "color still inh=9\n" +
      "noProto.kind=0\n"
    ));
});
