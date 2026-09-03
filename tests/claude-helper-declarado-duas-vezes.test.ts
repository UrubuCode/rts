// Um nome de helper declarado MAIS DO QUE UMA VEZ no mesmo corpo, em blocos
// irmaos — cada declaracao com o seu proprio corpo.
//
// PORQUE ESTE FICHEIRO EXISTE
//
// `emit/omit.rs` decide, uma vez por corpo e antes de o emitir, quais dos
// helpers desse corpo podem ser substituidos no sitio da chamada em vez de
// construidos como closure. Recolhe as declaracoes com `helper_bindings`, que
// desce a blocos irmaos, e guarda-as num mapa por NOME — onde a segunda
// declaracao sobrescreve a primeira. Todas as chamadas do corpo passavam
// entao a executar o corpo da ULTIMA declaracao.
//
// O comentario que autorizava isso dizia: "as duas clausulas acima provaram
// que toda chamada a este nome esta dentro deste corpo, portanto a declaracao
// em mao e a que toda chamada alcanca". As duas clausulas provam que o nome
// nunca e lido como valor e que nunca e capturado. Nenhuma delas prova que ha
// UMA declaracao.
//
// Reportado como issue #2617 sobre um descodificador de protobuf de ~950
// linhas, onde o efeito era um numero de campo a sair como se fosse o valor do
// campo — sem erro nenhum, so bytes errados. O relator nao conseguiu reduzir o
// caso, e a razao e que o minimo precisa de DOIS corpos DIFERENTES sob um nome
// so: com corpos identicos a colisao existe e nao se nota.
//
// A regra 11 do `crates/rts-codegen/README.md` manda que uma fixture destas
// afirme VALORES e nunca que uma substituicao aconteceu — uma que afirmasse a
// substituicao passaria a verde no dia em que o pass fosse desligado por
// engano, que e precisamente a falha que essa regra descreve.

import { describe, test, expect } from "rts:test";

// --- o minimo: dois corpos, um nome ---------------------------------------

function doisCorpos(): { a: number; b: number } {
  const out: { a: number; b: number } = { a: 0, b: 0 };
  {
    const nm = (k: number) => k + 1;
    out.a = nm(10);
  }
  {
    const nm = (k: number) => k * 3;
    out.b = nm(10);
  }
  return out;
}

// --- tres, para provar que nao e "a primeira ou a ultima" ------------------

function tresCorpos(): { a: number; b: number; c: number } {
  const out: { a: number; b: number; c: number } = { a: 0, b: 0, c: 0 };
  {
    const nm = (k: number) => k + 1;
    out.a = nm(10);
  }
  {
    const nm = (k: number) => k * 3;
    out.b = nm(10);
  }
  {
    const nm = (k: number) => k - 4;
    out.c = nm(10);
  }
  return out;
}

// --- o caso da issue: um le uma estrutura pela CHAVE, o outro devolve o
//     ARGUMENTO. E o par que torna o sintoma legivel — quando a chamada se
//     liga ao segundo, `numOf(3)` responde 3, que e a chave e nao o valor.

function chaveContraValor(): { a: number | undefined; b: number | undefined } {
  const out: { a: number | undefined; b: number | undefined } = {
    a: undefined,
    b: undefined,
  };
  {
    const sf = new Map<number, Array<number>>([
      [2, [7]],
      [3, [9]],
    ]);
    const numOf = (k: number) =>
      typeof sf.get(k)?.[0] === "number" ? (sf.get(k)![0] as number) : undefined;
    out.a = numOf(2);
    out.b = numOf(3);
  }
  {
    const numOf = (x: number | undefined) => (typeof x === "number" ? x : undefined);
    // A chamada existe para que esta declaracao seja um candidato a serio.
    // O valor nao interessa; o que interessa e o efeito dela sobre o bloco
    // acima, que e onde a asserção esta.
    if (numOf(42) !== 42) throw new Error("o segundo helper esta errado");
  }
  return out;
}

// --- ordem invertida: o que devolve o argumento vem PRIMEIRO ---------------

function ordemInvertida(): { a: number | undefined; b: number | undefined } {
  const out: { a: number | undefined; b: number | undefined } = {
    a: undefined,
    b: undefined,
  };
  {
    const numOf = (x: number | undefined) => (typeof x === "number" ? x : undefined);
    if (numOf(42) !== 42) throw new Error("o primeiro helper esta errado");
  }
  {
    const sf = new Map<number, Array<number>>([
      [2, [7]],
      [3, [9]],
    ]);
    const numOf = (k: number) =>
      typeof sf.get(k)?.[0] === "number" ? (sf.get(k)![0] as number) : undefined;
    out.a = numOf(2);
    out.b = numOf(3);
  }
  return out;
}

// --- cada declaracao captura uma variavel DIFERENTE ------------------------
//
// Aqui o sintoma nao e um valor errado mas um `ReferenceError`: o corpo da
// outra declaracao e emitido no ambiente deste bloco, onde o nome que ele
// captura nao existe. Um `try` prende-o como valor em vez de deixar a excecao
// levar o processo, para que o ficheiro continue mensuravel se voltar a
// partir-se.

function capturasDiferentes(): { a: number | string; b: number | string } {
  const out: { a: number | string; b: number | string } = { a: 0, b: 0 };
  {
    const p = 100;
    const nm = (k: number) => k + p;
    try {
      out.a = nm(1);
    } catch (e) {
      out.a = `lancou: ${e}`;
    }
  }
  {
    const q = 200;
    const nm = (k: number) => k * q;
    try {
      out.b = nm(2);
    } catch (e) {
      out.b = `lancou: ${e}`;
    }
  }
  return out;
}

// --- `let` responde como `const` -------------------------------------------

function comLet(): { a: number; b: number } {
  const out: { a: number; b: number } = { a: 0, b: 0 };
  {
    let nm = (k: number) => k + 1;
    out.a = nm(10);
  }
  {
    let nm = (k: number) => k * 3;
    out.b = nm(10);
  }
  return out;
}

// --- o controlo: nomes distintos. Tem de continuar certo depois da correcao,
//     senao a guarda desligou mais do que devia.

function nomesDistintos(): { a: number; b: number } {
  const out: { a: number; b: number } = { a: 0, b: 0 };
  {
    const n1 = (k: number) => k + 1;
    out.a = n1(10);
  }
  {
    const n2 = (k: number) => k * 3;
    out.b = n2(10);
  }
  return out;
}

// --- e o outro controlo: UMA so declaracao, chamada muitas vezes. E a forma
//     que a substituicao existe para acelerar, e tem de continuar a responder
//     bem.

function umaSoDeclaracao(): number {
  const nm = (k: number) => k * 2 + 1;
  let total = 0;
  for (let i = 0; i < 5; i++) total += nm(i);
  return total;
}

const dois = doisCorpos();
const tres = tresCorpos();
const chave = chaveContraValor();
const invertida = ordemInvertida();
const capturas = capturasDiferentes();
const let2 = comLet();
const distintos = nomesDistintos();
const uma = umaSoDeclaracao();

describe("helper com o mesmo nome declarado duas vezes no mesmo corpo (#2617)", () => {
  test("cada bloco corre o SEU corpo, nao o do outro", () => {
    expect(dois.a).toBe(11);
    expect(dois.b).toBe(30);
  });

  test("com tres declaracoes, cada uma responde a sua", () => {
    expect(tres.a).toBe(11);
    expect(tres.b).toBe(30);
    expect(tres.c).toBe(6);
  });

  test("o helper que le pela chave nao passa a devolver a chave", () => {
    expect(chave.a).toBe(7);
    expect(chave.b).toBe(9);
  });

  test("o mesmo com a ordem das declaracoes trocada", () => {
    expect(invertida.a).toBe(7);
    expect(invertida.b).toBe(9);
  });

  test("cada corpo ve a variavel que ELE captura", () => {
    expect(capturas.a).toBe(101);
    expect(capturas.b).toBe(400);
  });

  test("`let` responde como `const`", () => {
    expect(let2.a).toBe(11);
    expect(let2.b).toBe(30);
  });
});

describe("helper com o mesmo nome — os controlos", () => {
  test("nomes distintos continuam certos", () => {
    expect(distintos.a).toBe(11);
    expect(distintos.b).toBe(30);
  });

  test("uma so declaracao, chamada em ciclo, continua certa", () => {
    // 1 + 3 + 5 + 7 + 9
    expect(uma).toBe(25);
  });
});
