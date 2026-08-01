import { describe, test, expect } from "rts:test";

// A família `Error` CHAMADA SEM `new`.
//
// Pela spec, os oito construtores nativos de erro se comportam igual chamados
// com ou sem `new` — `Error(m)` faz exatamente o mesmo que `new Error(m)`. São
// os construtores no estilo função legado; uma `class` de usuário que estenda
// `Error` NÃO herda isso (uma classe ES2015 lança se chamada sem `new`), e por
// isso o motor só roteia esses oito.
//
// Antes: `Error("x")` era `call to unknown function \`Error\`` — um bail de
// COMPILAÇÃO, que derrubava o programa inteiro. As formas com `new` já
// funcionavam e estão aqui para garantir que não regrediram.
//
// Valores conferidos contra o Node. Pré-computado no top-level (ler propriedade
// de instância dentro do closure de `test()` esbarra num bug pré-existente de
// leitura de campo em arrow).

// ── sem `new` ───────────────────────────────────────────────────────────────
const semNew = Error("x");
const semNewMessage = semNew.message;
const semNewName = semNew.name;
const semNewInstance = semNew instanceof Error;
const semNewStack = typeof semNew.stack;
const semNewToString = semNew.toString();

// ── sem `new`, sem argumento: `message` é a string vazia ────────────────────
const semArgs = Error();
const semArgsMessage = semArgs.message;
const semArgsName = semArgs.name;
const semArgsToString = semArgs.toString();

// ── sem `new`, com o bag `{ cause }` do ES2022 ──────────────────────────────
const comCause = Error("x", { cause: 7 });
const comCauseValor = comCause.cause;
const comCauseMessage = comCause.message;

// ── com `new` (não pode regredir) ───────────────────────────────────────────
const comNew = new Error("x");
const comNewMessage = comNew.message;
const comNewName = comNew.name;

// ── subclasses primordiais, sem `new` ───────────────────────────────────────
const te = TypeError("t");
const teName = te.name;
const teMessage = te.message;
const teInstance = te instanceof Error;
const teToString = te.toString();

const re = RangeError("r");
const reName = re.name;
const reMessage = re.message;

const refe = ReferenceError("f");
const refeName = refe.name;
const refeMessage = refe.message;

const se = SyntaxError("s");
const seName = se.name;
const seMessage = se.message;

const ue = URIError("u");
const ueName = ue.name;
const ueMessage = ue.message;

const ee = EvalError("e");
const eeName = ee.name;
const eeMessage = ee.message;

// `AggregateError(errors, message)` — o primeiro argumento é a LISTA, não a
// mensagem (diferente das outras subclasses).
const ae = AggregateError([1, 2], "m");
const aeName = ae.name;
const aeMessage = ae.message;
const aeErrosLen = ae.errors.length;

// ── `throw` da forma sem `new` ──────────────────────────────────────────────
let lancadoMessage = "";
let lancadoName = "";
try {
  throw Error("boom");
} catch (err) {
  lancadoMessage = err.message;
  lancadoName = err.name;
}

let lancadoTeName = "";
try {
  throw TypeError("mau tipo");
} catch (err) {
  lancadoTeName = err.name;
}

// ── um local de mesmo nome SOMBREIA o primordial ────────────────────────────
function Error2(m: string): string {
  return "sombra:" + m;
}
const sombra = Error2("z");

// ── `Error` como VALOR (já funcionava; guarda de não-regressão) ─────────────
const ValorE = Error;
const viaValor = new ValorE("v");
const viaValorMessage = viaValor.message;

describe("Error chamável sem `new`", () => {
  test("Error(m) constrói igual a new Error(m)", () => {
    expect(semNewMessage).toBe("x");
    expect(semNewName).toBe("Error");
    expect(semNewInstance).toBe(true);
    expect(semNewStack).toBe("string");
    expect(semNewToString).toBe("Error: x");
  });

  test("Error() sem argumento tem message vazia", () => {
    expect(semArgsMessage).toBe("");
    expect(semArgsName).toBe("Error");
    // `toString()` sem mensagem devolve só o nome.
    expect(semArgsToString).toBe("Error");
  });

  test("Error(m, { cause }) propaga a causa", () => {
    expect(comCauseValor).toBe(7);
    expect(comCauseMessage).toBe("x");
  });

  test("a forma com `new` não regrediu", () => {
    expect(comNewMessage).toBe("x");
    expect(comNewName).toBe("Error");
  });

  test("as duas grafias produzem o mesmo", () => {
    expect(semNewMessage).toBe(comNewMessage);
    expect(semNewName).toBe(comNewName);
  });
});

describe("subclasses primordiais sem `new`", () => {
  test("TypeError", () => {
    expect(teName).toBe("TypeError");
    expect(teMessage).toBe("t");
    expect(teInstance).toBe(true);
    expect(teToString).toBe("TypeError: t");
  });

  test("RangeError / ReferenceError", () => {
    expect(reName).toBe("RangeError");
    expect(reMessage).toBe("r");
    expect(refeName).toBe("ReferenceError");
    expect(refeMessage).toBe("f");
  });

  test("SyntaxError / URIError / EvalError", () => {
    expect(seName).toBe("SyntaxError");
    expect(seMessage).toBe("s");
    expect(ueName).toBe("URIError");
    expect(ueMessage).toBe("u");
    expect(eeName).toBe("EvalError");
    expect(eeMessage).toBe("e");
  });

  test("AggregateError leva a lista no 1º argumento", () => {
    expect(aeName).toBe("AggregateError");
    expect(aeMessage).toBe("m");
    expect(aeErrosLen).toBe(2);
  });
});

describe("throw e sombreamento", () => {
  test("throw Error(m) sem new", () => {
    expect(lancadoMessage).toBe("boom");
    expect(lancadoName).toBe("Error");
  });

  test("throw TypeError(m) sem new", () => {
    expect(lancadoTeName).toBe("TypeError");
  });

  test("uma função de usuário de mesmo nome vence", () => {
    expect(sombra).toBe("sombra:z");
  });

  test("Error lido como valor continua construindo", () => {
    expect(viaValorMessage).toBe("v");
  });
});
