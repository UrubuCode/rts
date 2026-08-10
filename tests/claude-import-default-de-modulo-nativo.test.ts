import { describe, test, expect } from "rts:test";
import fs from "node:fs";
import * as star from "node:fs";
import semDefault from "./claude-import-default-fixture-sem.ts";
import comDefault, { marcador } from "./claude-import-default-fixture-com.ts";

// `import fs from "node:fs"` — a forma como praticamente todo código é escrito —
// ligava `fs` a `undefined`.
//
// Um módulo do host não tem `export default` para achar: `node:fs` é um objeto
// de funções construído em Rust, sem uma única declaração de export. A busca por
// `default` não achava nada e respondia `undefined`, então TODO membro lido
// depois vinha de nada.
//
// O que fazia isso caro não era o `undefined`, era ONDE ele falhava: não no
// import, mas na primeira chamada, como `fs.readFileSync is not a function` —
// que se lê como um método faltando, e não como o import que abriu o buraco.
//
// A regra que decide os dois lados está em `Registered`: um módulo do HOST
// responde a si mesmo por `default` (é o que o interop CommonJS do Node faz para
// exatamente estes especificadores), e um módulo COMPILADO sem `export default`
// continua respondendo `undefined`, porque isso é semântica ES e não um buraco.

describe("import default", () => {
  test("um módulo do host responde a si mesmo", () => {
    expect(typeof fs.readFileSync).toBe("function");
    expect(typeof fs.writeFileSync).toBe("function");
    expect(typeof fs.existsSync).toBe("function");
  });

  test("é o MESMO objeto que `import * as` entrega", () => {
    // Não uma cópia: uma segunda tabela de membros divergiria da primeira no
    // dia em que um dos dois caminhos ganhasse um nome.
    expect(fs.readFileSync).toBe(star.readFileSync);
  });

  test("um módulo compilado SEM export default ainda responde undefined", () => {
    // A metade que o atalho fácil teria quebrado: responder o namespace aqui
    // faria um default faltando parecer um import funcionando.
    expect(semDefault).toBe(undefined);
  });

  test("um módulo compilado COM export default responde o valor", () => {
    expect(comDefault).toBe(42);
    expect(marcador).toBe("nomeado");
  });
});
