import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Indice do primeiro match + numero de matches, agora sobre RegExp.
// regex.find_at -> String.prototype.search (mesmo indice, -1 se ausente);
// regex.match_count -> match(/.../g).length, que exige a flag g porque
// sem ela match() devolve so' o primeiro. regex.free nao tem par: RegExp
// e' coletado, nao possui handle a liberar.

const subject = "abc 123 def 456";
const idx = subject.search(/[0-9]+/);
const cnt = (subject.match(/[0-9]+/g) || []).length;
print(`${idx}`); // 4
print(`${cnt}`); // 2

describe("fixture:regex_find_at", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("4\n2\n");
  });
});
