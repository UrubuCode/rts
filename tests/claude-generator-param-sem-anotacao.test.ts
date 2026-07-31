import { describe, test, expect } from "rts:test";

// Um generator LAZY (state-machine — corpo com loop contendo `yield`) cujo
// parâmetro NÃO tem anotação de tipo produzia um iterador VAZIO, silenciosamente:
// `.next()` lia `undefined`, spread devolvia `[]`, `for-of` não iterava.
//
// A causa (issue #2044) é a mesma classe do #2042, noutro ponto. `sig.rs` tem uma
// regra que força o RETORNO a `Tagged` quando qualquer parâmetro é `Tagged` — e um
// parâmetro sem anotação é exatamente isso. Mas o ctor de um generator lazy não
// devolve um valor: devolve o HANDLE opaco da `Entry::GenState`, que precisa
// continuar `Int64`. Forçado a `Tagged`, o retorno era coagido com
// `fcvt_from_sint` e o handle virava um double comum — identidade destruída.
//
// No IR o `return` do ctor mostrava a diferença exata:
//   sem anotação:  v6 = fcvt_from_sint.f64 v3   →  return v10   (handle → double)
//   com anotação:  return v3                    (o handle cru)
//
// `is_lazy_gen` já era calculado ali, mas só o caminho EAGER ajustava o `ret`; o
// lazy não. A correção fixa `ret = Int64` para o ctor lazy, depois das demais
// regras. O boxing para consumidores dinâmicos continua no CALL SITE (#2042).
//
// Importa porque JS de bundle é minificado e NÃO tem anotação de tipo — a forma
// que aparece em código real é justamente a que falhava.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function* semTipo(start, count) { for (let i = 0; i < count; i++) yield start + i; }
function* comTipo(start: number, count: number) { for (let i = 0; i < count; i++) yield start + i; }
function* semParam() { let n = 1; while (n <= 3) { yield n; n = n + 1; } }
function* comDefault(start = 1) { let n = start; while (n < start + 3) { yield n; n = n + 1; } }
function* doisParams(a, b) { let i = 0; while (i < 2) { yield a + b + i; i = i + 1; } }
function* strParam(p) { let i = 0; while (i < 2) { yield p + i; i = i + 1; } }
function* eagerParam(s) { yield s; yield s + 1; }

const viaSpread = [...semTipo(10, 3)].join(",");
const viaNext = semTipo(10, 3).next().value;

let somaForOf = 0;
for (const x of semTipo(1, 3)) { somaForOf = somaForOf + x; }

// consumido através de uma borda dinâmica (combina com o fix da #2042)
const viaBorda = [semTipo(10, 3)][0].next().value;

// o cursor tem de avançar: mesmo iterador, não uma cópia
const compartilhado = semTipo(100, 3);
const sequencia =
  compartilhado.next().value + compartilhado.next().value + compartilhado.next().value;

const comAnotacao = [...comTipo(10, 3)].join(",");
const semParametro = [...semParam()].join(",");
const defaultOmitido = [...comDefault()].join(",");
const defaultPassado = [...comDefault(5)].join(",");
const doisSemTipo = [...doisParams(1, 2)].join(",");
const stringParam = [...strParam("x")].join(",");
const eagerComParam = [...eagerParam(7)].join(",");

// generator infinito só é possível no caminho lazy (o eager materializaria tudo)
function* infinito(start = 1) { let n = start; while (true) { yield n; n = n + 1; } }
const doInfinito = infinito();
const infinitoTresValores =
  doInfinito.next().value + doInfinito.next().value + doInfinito.next().value;

describe("generator lazy com parâmetro sem anotação de tipo", () => {
  test("spread produz os valores", () => {
    expect(viaSpread).toBe("10,11,12");
  });

  test("next() produz o primeiro valor", () => {
    expect(viaNext).toBe(10);
  });

  test("for-of itera", () => {
    expect(somaForOf).toBe(6);
  });

  test("atravessa borda dinâmica (com o fix da #2042)", () => {
    expect(viaBorda).toBe(10);
  });

  test("cursor avança entre chamadas", () => {
    expect(sequencia).toBe(303);
  });

  test("dois parâmetros sem anotação", () => {
    expect(doisSemTipo).toBe("3,4");
  });

  test("parâmetro string sem anotação", () => {
    expect(stringParam).toBe("x0,x1");
  });

  test("parâmetro com default, omitido", () => {
    expect(defaultOmitido).toBe("1,2,3");
  });

  test("parâmetro com default, passado", () => {
    expect(defaultPassado).toBe("5,6,7");
  });

  test("generator infinito rende sob demanda", () => {
    expect(infinitoTresValores).toBe(6);
  });
});

describe("não-regressões dos demais caminhos de generator", () => {
  test("parâmetro ANOTADO não regrediu", () => {
    expect(comAnotacao).toBe("10,11,12");
  });

  test("generator SEM parâmetro não regrediu", () => {
    expect(semParametro).toBe("1,2,3");
  });

  test("generator EAGER com parâmetro não regrediu", () => {
    expect(eagerComParam).toBe("7,8");
  });
});
