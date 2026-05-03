import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #452: garantir que stmts apos `new Error()` sem args nao sao
// "engolidas" (regressao original: linha sumia inteira).

const before = "x";
const e = new Error();
const after = "y";

print(before);
print(`mid=[${e.message}]`);
print(after);

// Mesma verificacao em loop (cada iter cria um Error novo)
for (let i = 0; i < 3; i++) {
  const ei = new Error();
  print(`it${i}=[${ei.message}]`);
}

// Conditional: ramo com new Error() nao deve perder stmt subsequente
let cond = true;
if (cond) {
  const e2 = new Error();
  print(`cond=[${e2.message}]`);
  print("post-cond");
}

describe("ctor sem args nao engole stmt seguinte (#452)", () => {
  test("sequencias antes/depois preservadas", () => {
    expect(__rtsCapturedOutput).toBe(
      "x\n" +
      "mid=[]\n" +
      "y\n" +
      "it0=[]\n" +
      "it1=[]\n" +
      "it2=[]\n" +
      "cond=[]\n" +
      "post-cond\n"
    );
  });
});
