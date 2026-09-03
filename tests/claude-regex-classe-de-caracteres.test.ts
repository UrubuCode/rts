// A sintaxe de uma classe de caracteres, onde o JavaScript e o motor `regex`
// do Rust discordam.
//
// PORQUE ESTE FICHEIRO EXISTE
//
// Dentro de `[...]`, o JavaScript nao da significado nenhum a `[`, `&` nem `~`:
// cada um e um membro vulgar do conjunto e nao precisa de escape. O `regex` do
// Rust le o primeiro como uma classe ANINHADA e os outros dois, dobrados, como
// operadores de conjunto (`&&` intersecao, `~~` diferenca simetrica). Os mesmos
// tres caracteres escrevem conjuntos diferentes nas duas linguagens.
//
// Isso dava duas falhas, e so uma era visivel:
//
//  - `/[a[b]/` era RECUSADO com `SyntaxError` — issue #2612, encontrada porque
//    o `get-intrinsic` (dependencia transitiva de centenas de pacotes npm)
//    traz `/[^%.[\]]+|.../` e rebentava ao carregar.
//  - `/[a&&b]/` COMPILAVA, como a intersecao de `{a}` com `{b}`, que e vazia —
//    e `/[a&&b]/.test("a")` respondia `false` onde todo o outro motor responde
//    `true`. Ninguem tinha reportado, porque uma resposta errada sem erro nao
//    se ve.
//
// Todas as afirmacoes aqui foram confirmadas contra o Node v20.19.5 desta
// maquina. Onde o RTS diverge de proposito, o teste afirma o que o RTS faz e o
// comentario diz porque.

import { describe, test, expect } from "rts:test";

// Um `new RegExp` que rebenta leva o processo, entao a construcao e sempre
// embrulhada: um padrao recusado le-se como `null` e o teste falha a dizer o
// que devia ter compilado, em vez de matar o ficheiro e levar os outros.
function re(pattern: string, flags?: string): RegExp | null {
  try {
    return flags === undefined ? new RegExp(pattern) : new RegExp(pattern, flags);
  } catch (e) {
    return null;
  }
}

function testa(pattern: string, subject: string): boolean | string {
  const r = re(pattern);
  if (r === null) return "RECUSADO";
  return r.test(subject);
}

describe("classe de caracteres — o colchete aberto (#2612)", () => {
  test("um `[` dentro da classe e um membro vulgar", () => {
    expect(testa("[a[b]", "a")).toBe(true);
    expect(testa("[a[b]", "[")).toBe(true);
    expect(testa("[a[b]", "b")).toBe(true);
    expect(testa("[a[b]", "z")).toBe(false);
  });

  test("o mesmo numa classe negada", () => {
    expect(testa("[^a[b]", "a")).toBe(false);
    expect(testa("[^a[b]", "[")).toBe(false);
    expect(testa("[^a[b]", "z")).toBe(true);
  });

  test("uma classe que so tem o colchete", () => {
    expect(testa("[[]", "[")).toBe(true);
    expect(testa("[[]", "a")).toBe(false);
  });

  test("escapado continua a funcionar, e nao e escapado duas vezes", () => {
    expect(testa("[a\\[b]", "[")).toBe(true);
    expect(testa("[a\\[b]", "a")).toBe(true);
    expect(testa("[a\\[b]", "z")).toBe(false);
  });

  test("misturado com um intervalo", () => {
    expect(testa("[a-z[]", "m")).toBe(true);
    expect(testa("[a-z[]", "[")).toBe(true);
    expect(testa("[a-z[]", "0")).toBe(false);
  });

  test("o padrao do `get-intrinsic`, que foi como isto apareceu", () => {
    const r = re("[^%.[\\]]+|\\[(?:(-?\\d+(?:\\.\\d+)?)|([\"'])((?:(?!\\2)[^\\\\]|\\\\.)*?)\\2)\\]", "g");
    expect(r !== null).toBe(true);
    // E funciona: parte `a.b[0]` nos pedacos que a biblioteca espera.
    const achados = "a.b[0]".match(/[^%.[\]]+|\[(?:(-?\d+(?:\.\d+)?)|(["'])((?:(?!\2)[^\\]|\\.)*?)\2)\]/g);
    expect(achados !== null).toBe(true);
    expect(achados === null ? -1 : achados.length).toBe(3);
  });
});

describe("classe de caracteres — os operadores de conjunto do Rust", () => {
  // Estes tres compilavam e respondiam o conjunto ERRADO. Sao a metade que
  // ninguem tinha reportado.
  test("`&&` e dois membros literais, nao uma intersecao", () => {
    expect(testa("[a&&b]", "a")).toBe(true);
    expect(testa("[a&&b]", "&")).toBe(true);
    expect(testa("[a&&b]", "b")).toBe(true);
    expect(testa("[a&&b]", "z")).toBe(false);
  });

  test("`~~` e dois membros literais, nao uma diferenca simetrica", () => {
    expect(testa("[a~~b]", "a")).toBe(true);
    expect(testa("[a~~b]", "~")).toBe(true);
    expect(testa("[a~~b]", "b")).toBe(true);
    expect(testa("[a~~b]", "z")).toBe(false);
  });

  test("um `&` ou `~` sozinho tambem e literal", () => {
    expect(testa("[a&b]", "&")).toBe(true);
    expect(testa("[a~b]", "~")).toBe(true);
  });

  test("um intervalo ao lado de um operador continua a ser um intervalo", () => {
    expect(testa("[a-z&&0-9]", "m")).toBe(true);
    expect(testa("[a-z&&0-9]", "5")).toBe(true);
    expect(testa("[a-z&&0-9]", "&")).toBe(true);
    expect(testa("[a-z&&0-9]", "_")).toBe(false);
  });
});

describe("classe de caracteres — o que NAO mudou", () => {
  test("fora de uma classe os tres caracteres sao o que sempre foram", () => {
    // `[` fora de uma classe ABRE uma classe.
    expect(testa("a[bc]d", "abd")).toBe(true);
    expect(testa("a&&b", "a&&b")).toBe(true);
    expect(testa("a~b", "a~b")).toBe(true);
  });

  test("um intervalo vulgar", () => {
    expect(testa("[a-z]", "m")).toBe(true);
    expect(testa("[a-z]", "5")).toBe(false);
    expect(testa("[0-9a-fA-F]", "e")).toBe(true);
  });

  test("as duas classes vazias que este motor ja traduzia", () => {
    // `[]` nao casa nada e `[^]` casa tudo — legais em JavaScript, recusadas
    // pelo motor do Rust, traduzidas ha muito.
    expect(testa("[]", "a")).toBe(false);
    expect(testa("[^]", "a")).toBe(true);
    expect(testa("[^]", "\n")).toBe(true);
  });

  test("um `]` escapado nao fecha a classe", () => {
    expect(testa("[a\\]b]", "]")).toBe(true);
    expect(testa("[a\\]b]", "b")).toBe(true);
    expect(testa("[a\\]b]", "z")).toBe(false);
  });

  test("as classes com nome continuam a responder", () => {
    expect(testa("[\\d]", "5")).toBe(true);
    expect(testa("[\\w]", "_")).toBe(true);
    expect(testa("[\\s]", " ")).toBe(true);
    expect(testa("[^\\d]", "a")).toBe(true);
  });

  test("o ponto dentro de uma classe e um ponto", () => {
    expect(testa("[.]", ".")).toBe(true);
    expect(testa("[.]", "a")).toBe(false);
  });
});

describe("classe de caracteres — com flags", () => {
  test("`i` sobre uma classe que tem um colchete", () => {
    const r = re("[a[b]", "i");
    expect(r !== null).toBe(true);
    expect(r === null ? false : r.test("A")).toBe(true);
    expect(r === null ? false : r.test("[")).toBe(true);
  });

  test("`g` conta todas as ocorrencias", () => {
    const achados = "a[b[c".match(/[a[]/g);
    expect(achados === null ? -1 : achados.length).toBe(3);
  });

  test("`s` nao muda o que uma classe significa", () => {
    const r = re("[a&&b]", "s");
    expect(r !== null).toBe(true);
    expect(r === null ? false : r.test("&")).toBe(true);
  });
});

describe("classe de caracteres — a divergencia deixada de proposito", () => {
  // O `-` NAO e escapado: e um intervalo nas duas linguagens, e escapar todos
  // destruiria `[a-z]`. So o `--` dobrado diverge — o Rust le diferenca de
  // conjuntos, o JavaScript le o intervalo `a` ate `-` e recusa-o por estar
  // fora de ordem. E um padrao que o JavaScript rejeita, portanto nada de
  // correto depende de nenhuma das duas leituras: o RTS aceitar e uma frouxidao
  // e nao uma resposta errada.
  //
  // Este teste afirma o que o RTS FAZ, com a divergencia dita, para que uma
  // mudanca futura apareca em vez de passar despercebida.
  test("`[a--b]` e aceite aqui e recusado pelo Node", () => {
    expect(re("[a--b]") !== null).toBe(true);
  });
});
