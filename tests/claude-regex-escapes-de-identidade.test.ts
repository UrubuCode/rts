// Os escapes que o motor `regex` do Rust gasta e o JavaScript le como uma
// letra vulgar.
//
// PORQUE ESTE FICHEIRO EXISTE
//
// Fora da flag `u`, o `IdentityEscape` do JavaScript diz que uma barra antes de
// um caracter que ele nao reconhece E esse caracter. Medido contra o Node
// v20.19.5 desta maquina, e nao lido da gramatica: as 32 letras que o RTS passou
// a tratar assim respondem todas `true` a `new RegExp(barra + letra).test(letra)`
// e nenhuma responde outra coisa.
//
// O Rust gasta varias: `\A` e inicio-de-texto, `\z` e `\Z` sao fim-de-texto,
// `\q{...}` e uma cadeia literal, `\p{...}` uma propriedade Unicode. Os mesmos
// dois caracteres eram uma letra numa linguagem e uma ancora na outra.
//
// Isso dava falhas dos dois tipos, e so um era visivel:
//
//  - `\A` e `\z` COMPILAVAM, como ancoras, entao `/\A/.test("a")` respondia
//    `true` onde o Node responde `false` — errado, sem erro nenhum.
//  - `\Z`, `\q{ab}` e `\k<n>` sem grupo nomeado eram RECUSADOS, com
//    `SyntaxError` sobre um padrao que todo o outro motor aceita.
//
// Companheiro de `claude-regex-classe-de-caracteres.test.ts`, que prende a
// sintaxe DENTRO de `[...]` (a issue #2612 e os operadores de conjunto).

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

describe("uma barra antes de uma letra que o Rust gasta", () => {
  test("`\\A` e `\\z` sao letras, nao ancoras", () => {
    expect(testa("\\A", "A")).toBe(true);
    expect(testa("\\A", "a")).toBe(false);
    expect(testa("\\z", "z")).toBe(true);
    expect(testa("\\z", "a")).toBe(false);
  });

  test("`\\Z` e `\\q` eram recusados", () => {
    expect(testa("\\Z", "Z")).toBe(true);
    expect(testa("\\Z", "a")).toBe(false);
    expect(testa("[\\q{ab}]", "q")).toBe(true);
    expect(testa("[\\q{ab}]", "{")).toBe(true);
  });

  test("uma letra qualquer do conjunto responde por si", () => {
    expect(testa("\\e", "e")).toBe(true);
    expect(testa("\\h", "h")).toBe(true);
    expect(testa("\\R", "R")).toBe(true);
    expect(testa("\\X", "X")).toBe(true);
    expect(testa("\\N", "N")).toBe(true);
    expect(testa("\\K", "K")).toBe(true);
  });
});

describe("o que a reescrita NAO pode tocar", () => {
  test("os escapes que o JavaScript reconhece ficam como estao", () => {
    expect(testa("\\d", "5")).toBe(true);
    expect(testa("\\d", "a")).toBe(false);
    expect(testa("\\w", "_")).toBe(true);
    expect(testa("\\s", " ")).toBe(true);
    expect(testa("\\S", "a")).toBe(true);
    expect(testa("\\n", "\n")).toBe(true);
    expect(testa("\\t", "\t")).toBe(true);
  });

  test("um metacaracter escapado continua escapado", () => {
    expect(testa("\\.", ".")).toBe(true);
    expect(testa("\\.", "a")).toBe(false);
    expect(testa("\\*", "*")).toBe(true);
    expect(testa("\\[", "[")).toBe(true);
    expect(testa("\\$", "$")).toBe(true);
  });

  test("uma retro-referencia mantem o numero", () => {
    // Renumerar uma seria exatamente a resposta errada em silencio que esta
    // reescrita existe para remover.
    expect(testa("(a)\\1", "aa")).toBe(true);
    expect(testa("(a)\\1", "ab")).toBe(false);
    expect(testa("(a)(b)\\2", "abb")).toBe(true);
  });

  test("as fronteiras de palavra continuam a ser fronteiras", () => {
    expect(testa("a\\b", "a b")).toBe(true);
    expect(testa("a\\Bb", "ab")).toBe(true);
  });
});

describe("`\\k<n>` so e retro-referencia quando existe grupo com nome", () => {
  test("com grupo nomeado, e uma retro-referencia", () => {
    expect(testa("(?<n>a)\\k<n>", "aa")).toBe(true);
    expect(testa("(?<n>a)\\k<n>", "ab")).toBe(false);
  });

  test("sem grupo nomeado era recusado, e o Annex B le a letra", () => {
    expect(re("\\k<n>") !== null).toBe(true);
    expect(testa("\\k<n>", "k")).toBe(false);
    expect(testa("\\k<n>", "k<n>")).toBe(true);
  });

  test("um lookbehind nao e um grupo nomeado, e abre com os mesmos caracteres", () => {
    // A razao de a verificacao perguntar mais do que "contem `(?<`".
    expect(testa("(?<=a)b", "ab")).toBe(true);
    expect(testa("(?<!a)b", "cb")).toBe(true);
    expect(testa("(?<!a)b", "ab")).toBe(false);
    expect(re("(?<=a)\\k<n>") !== null).toBe(true);
  });
});

describe("o escape de propriedade, e a flag que o decide", () => {
  test("sem `u`, sao letras soltas", () => {
    // Respondia `true` porque o Rust lia a propriedade nos dois casos; o Node
    // sem `u` le a classe com `p`, `{`, `L`, `}` la dentro.
    expect(testa("[\\p{L}]", "a")).toBe(false);
    expect(testa("[\\p{L}]", "p")).toBe(true);
    expect(testa("[\\p{L}]", "{")).toBe(true);
    expect(testa("[\\p{L}]", "L")).toBe(true);
  });

  test("com `u` e uma propriedade nas duas linguagens", () => {
    const r = re("\\p{L}", "u");
    expect(r !== null).toBe(true);
    expect(r === null ? false : r.test("a")).toBe(true);
    expect(r === null ? false : r.test("1")).toBe(false);
  });

  test("a negada, tambem", () => {
    const r = re("\\P{L}", "u");
    expect(r !== null).toBe(true);
    expect(r === null ? false : r.test("1")).toBe(true);
  });
});

describe("o traco ao lado de um escape de conjunto", () => {
  // Um intervalo precisa de dois caracteres unicos, entao um traco encostado a
  // `\d` so pode ser ele proprio. O Rust recusava o padrao inteiro.
  test("`[\\w-\\d]` era recusado, e casa as tres coisas", () => {
    expect(testa("[\\w-\\d]", "-")).toBe(true);
    expect(testa("[\\w-\\d]", "a")).toBe(true);
    expect(testa("[\\w-\\d]", "5")).toBe(true);
  });

  test("de qualquer um dos lados do traco", () => {
    expect(testa("[\\d-z]", "-")).toBe(true);
    expect(testa("[\\d-z]", "5")).toBe(true);
    expect(testa("[a-\\d]", "-")).toBe(true);
    expect(testa("[a-\\d]", "a")).toBe(true);
  });

  test("um intervalo a serio continua a ser um intervalo", () => {
    expect(testa("[a-z]", "m")).toBe(true);
    expect(testa("[a-z]", "5")).toBe(false);
    expect(testa("[a-z0-9]", "7")).toBe(true);
    expect(testa("[a-z0-9]", "_")).toBe(false);
  });

  test("um traco no fim ou no principio ja era literal", () => {
    expect(testa("[-a]", "-")).toBe(true);
    expect(testa("[a-]", "-")).toBe(true);
    expect(testa("[a-z-]", "-")).toBe(true);
    expect(testa("[a-z-]", "m")).toBe(true);
  });
});

describe("o NUL, que o Rust recusava dentro de uma classe", () => {
  // O padrao e montado por concatenacao de proposito: um `\0` escrito a letra
  // na fonte e um escape octal legado, que o parser recusa em strict mode. O
  // que este teste prende e a regex, nao como o TypeScript soletra uma barra.
  const BARRA = String.fromCharCode(92);
  const padraoNul = "[" + BARRA + "0]";
  const soltoNul = BARRA + "0";

  test("uma classe com o escape NUL compila e casa o caracter", () => {
    expect(re(padraoNul) !== null).toBe(true);
    expect(testa(padraoNul, String.fromCharCode(0))).toBe(true);
    expect(testa(padraoNul, "0")).toBe(false);
  });

  test("e fora de uma classe tambem", () => {
    expect(re(soltoNul) !== null).toBe(true);
    expect(testa(soltoNul, String.fromCharCode(0))).toBe(true);
  });
});

describe("as divergencias que ficam, ditas por nome", () => {
  // Nenhuma e uma resposta errada em codigo correto: uma e a recusa deliberada
  // que o modulo ja documentava, e as outras duas sao frouxidoes sobre padroes
  // que o proprio JavaScript rejeita. Estao presas por um teste para que uma
  // mudanca futura apareca em vez de passar despercebida.
  test("`\\cX` continua recusado — a recusa que o modulo ja nomeava", () => {
    expect(re("[\\cA]")).toBe(null);
  });

  test("`a**` e aceite aqui e recusado pelo Node", () => {
    expect(re("a**") !== null).toBe(true);
  });

  test("`a{,3}` e um quantificador aqui e texto literal no Node", () => {
    expect(testa("a{,3}", "a{,3}")).toBe(true);
  });
});
