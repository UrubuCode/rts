import { describe, test, expect } from "rts:test";
import { gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #333: `obj?.method()` (OptChain Call em Member) disparava Verifier
// "invalid block reference" — fix em operators.rs achata o duplo
// guard em um null-check unico sobre obj.

// Caso 1: obj null — retorna 0 sem chamar
const deep1: any = null;
const r1 = deep1?.getValue();
const h1 = gc.string_from_i64(r1 === null || r1 === 0 ? 0 : 1);
print(h1); gc.string_free(h1);

// Caso 2: obj nao-null com metodo presente
function makeObj(): any {
  const m = new Map<string, i64>();
  m.set("getValue", 99 as any);
  return m;
}
// Em RTS, map_get em var.method() retorna funcptr — esse caso requer
// que a chave esteja no mapa. Como nao temos closures-em-mapa direto
// aqui, validamos so' o caminho null (caso comum em prod).

describe("fixture:optchain_call_member", () => {
  test("obj?.method() em obj null nao crasha + Verifier passes", () => {
    expect(__rtsCapturedOutput).toBe("0\n");
  });
});
