import { describe, test, expect } from "rts:test";
import comDados from "./claude-export-default-fixture-objeto.ts";

// `export default { n: 1 }` matava o MÓDULO INTEIRO.
//
// A análise de escape achata `const x = { a: 1 }` em locais soltos quando nada
// faz o objeto escapar — uma otimização correta, e a publicação do módulo era
// "nada": ela é emitida DEPOIS do corpo, por um caminho que o scan do corpo não
// enxerga. Então o objeto era dissolvido, o `@@default` ficava ligado a coisa
// nenhuma, e a publicação lia o nome como um GLOBAL: `ReferenceError: @@default
// is not defined`. Quem importava via `cannot resolve module`, que nomeia o
// problema errado.
//
// Só um literal SEM função chegava a esse caminho — um objeto com método não é
// achatável — e é por isso que o defeito parecia aleatório: `{ m(){} }`
// funcionava, `{ n: 1 }` não, e `const o = { n: 1 }; export default o` também
// funcionava.
//
// A correção é a mesma que `rest` e os imports já tinham: um nome que a
// publicação lê entra em `captured`, porque uma menção que a análise não
// encontra continua sendo uma leitura.

describe("export default de objeto literal", () => {
  test("um literal só com dados sobrevive à análise de escape", () => {
    expect(typeof comDados).toBe("object");
    expect(comDados.n).toBe(1);
  });
});
