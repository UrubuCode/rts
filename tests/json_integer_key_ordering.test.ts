import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// ECMA-262 OrdinaryOwnPropertyKeys: integer keys ascendentes primeiro,
// depois demais string keys em ordem de insercao.
const obj: any = {};
obj["20"] = "twenty";
obj["3"] = "three";
obj["aa"] = "aa";
obj["10"] = "ten";
obj["b"] = "b";

print("json=" + JSON.stringify(obj));

// Mesmo objeto com keys integer puras: ordenadas como numero.
const ints: any = {};
ints["100"] = 1;
ints["2"] = 2;
ints["50"] = 3;
print("ints=" + JSON.stringify(ints));

describe("JSON.stringify integer-key ordering (#743)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      'json={"3":"three","10":"ten","20":"twenty","aa":"aa","b":"b"}\n' +
      'ints={"2":2,"50":3,"100":1}\n'
    )
  );
});
