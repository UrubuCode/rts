import { describe, test, expect } from "rts:test";
import f from "rts:fmt";
import e from "rts:env";

// O NOME escolhido num `import ... from "rts:<ns>"` podia QUEBRAR o prelude.
//
// `import f from "rts:fetch"` fazia o programa inteiro morrer com
// "builtin import `write` from `rts:fetch`: no matching namespace function" —
// um erro apontando para código que o usuário nunca escreveu. A causa: o
// prelude `streams.ts` tem
//
//     const f = this.__fwd;
//     if (f !== null) { f.write(chunk); return; }
//
// e a resolução de método consultava a tabela de imports ANTES de checar se o
// nome era um LOCAL. O `f` local do prelude era sombreado pelo import do
// usuário, e `f.write(...)` virava uma busca de membro no namespace.
//
// Isso é o INVERSO do escopo de JS: um `const` local sombreia o import, nunca o
// contrário. E importa muito: nome de uma letra é o que código minificado usa.
//
// Este teste importa com nomes de UMA LETRA que colidem com locais do prelude —
// antes bastava para derrubar o arquivo inteiro.

const hex = f.fmt_hex(255);
const temPath = e.get_var("PATH").length > 0;

// um local com o MESMO nome do import sombreia dentro da função
function localSombreia(): string {
  const f = { marca: "local" };
  return f.marca;
}
const doLocal = localSombreia();

describe("import não sombreia local do prelude", () => {
  test("import de uma letra (f) não quebra o prelude", () => {
    expect(hex).toBe("0xff");
  });

  test("segundo import de uma letra (e) coexiste", () => {
    expect(temPath).toBe(true);
  });

  test("local de mesmo nome vence o import (escopo de JS)", () => {
    expect(doLocal).toBe("local");
  });
});
