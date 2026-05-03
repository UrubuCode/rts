import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #452: ctor de Error/TypeError/etc com 0 args precisa retornar
// instancia com message="" em vez de "absorver" a stmt seguinte.

const e = new Error();
print(`a=[${e.message}]`);

const t = new TypeError();
print(`b=[${t.message}]`);
print(`bn=${t.name}`);

const r = new RangeError();
print(`c=[${r.message}]`);
print(`cn=${r.name}`);

const ref = new ReferenceError();
print(`d=[${ref.message}]`);
print(`dn=${ref.name}`);

const syn = new SyntaxError();
print(`e=[${syn.message}]`);
print(`en=${syn.name}`);

describe("error sem args (#452)", () => {
  test("Error() => message vazia, name preservado", () => {
    expect(__rtsCapturedOutput).toBe(
      "a=[]\n" +
      "b=[]\nbn=TypeError\n" +
      "c=[]\ncn=RangeError\n" +
      "d=[]\ndn=ReferenceError\n" +
      "e=[]\nen=SyntaxError\n"
    );
  });
});
