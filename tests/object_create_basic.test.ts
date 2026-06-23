import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264) Object.create(proto) — modelo novo: o resultado é um objeto cujo
// [[Prototype]] é `proto`. Lookup de propriedade segue a cadeia quando a key não
// é own; uma atribuição direta cria/sobrescreve a própria (own), sombreando o
// proto. (Reescrito do modelo antigo objeto=map handle para o modelo de objeto
// shape-slot do motor novo — sem `collections.map_*`.)

const proto: any = { kind: 7, color: 9 };

const inst: any = Object.create(proto);
print("nonzero=" + (inst !== null));

// Lookups herdados do proto.
print("kind=" + inst.kind);
print("color=" + inst.color);

// Adiciona own + sobrescreve um campo do proto (own sombreia).
inst.id = 99;
inst.kind = 1;

print("id=" + inst.id);
print("kind own=" + inst.kind);
print("color still inh=" + inst.color);

// Object.create(null) — sem prototype: lookup ausente é undefined.
const noProto: any = Object.create(null);
print("noProto.kind=" + noProto.kind);

describe("Object.create + chain (#264)", () => {
  test("aloca, herda do proto, own sobrescreve, sem proto sem chain", () =>
    expect(__rtsCapturedOutput).toBe(
      "nonzero=true\n" +
      "kind=7\n" +
      "color=9\n" +
      "id=99\n" +
      "kind own=1\n" +
      "color still inh=9\n" +
      "noProto.kind=undefined\n"
    ));
});
