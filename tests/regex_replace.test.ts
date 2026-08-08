import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// replace_all / replace, agora sobre RegExp. O padrao passou a existir
// em duas formas porque em JS "todas as ocorrencias" e' a flag g do
// padrao, nao um metodo separado: /foo/g para o replace_all, /foo/ para
// o replace de uma so' ocorrencia.

const h1 = "foo bar foo baz".replace(/foo/g, "X");
print(h1); // X bar X baz
const h2 = "foo and foo".replace(/foo/, "Y");
print(h2); // Y and foo

describe("fixture:regex_replace", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("X bar X baz\nY and foo\n");
  });
});
