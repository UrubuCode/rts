import { describe, test, expect } from "rts:test";

// `new RegExp(x)` com `x` que NÃO é string literal.
//
// Antes era um bail explícito ("a regex-from-regex copy / coercion is a later
// increment") e derrubava scripts reais de página: bundle minificado constrói
// regex a partir de variável o tempo todo.
//
// A armadilha, e o motivo deste teste existir: coagir com `ToString(re)` PARECE
// certo e está errado — devolve `/ab+c/` COM as barras, e compilar isso produz
// uma regex que casa o texto literal "/ab+c/". A cópia deixaria de casar o que a
// original casa, SEM erro nenhum. A spec manda usar o `source`/`flags` da
// original, que é o que `re_compile` faz agora.
//
// Valores conferidos contra o Node. Pré-computado no top-level (regra do
// projeto: método dentro de test() pode perder handle pro GC).

// ── cópia de uma RegExp: tem de casar o que a original casa ────────────────
const orig = /ab+c/;
const copia = new RegExp(orig);
const copiaCasa = copia.test("abbc");
const copiaNaoCasa = copia.test("axc");
const copiaSource = copia.source;

// ── flags são HERDADAS da original quando não há 2º argumento ──────────────
const comFlags = /x/gi;
const copiaFlags = new RegExp(comFlags);
const herdouSource = copiaFlags.source;
const herdouFlags = copiaFlags.flags;

// ── flags EXPLÍCITAS vencem as da original (spec) ──────────────────────────
const sobrescreve = new RegExp(/a/i, "g");
const sobrescreveSource = sobrescreve.source;
const sobrescreveFlags = sobrescreve.flags;

// ── pattern vindo de VARIÁVEL string (o caso comum em bundle) ──────────────
const pat = "a.c";
const deVar = new RegExp(pat);
const deVarCasa = deVar.test("axc");

// ── string literal continua funcionando (não pode regredir) ────────────────
const literal = new RegExp("a", "g");
const literalSource = literal.source;
const literalFlags = literal.flags;

// ── flags vindas de variável ───────────────────────────────────────────────
const fl = "gi";
const flagsDeVar = new RegExp("x", fl);
const flagsDeVarFlags = flagsDeVar.flags;

describe("new RegExp com pattern não-literal", () => {
  test("cópia de RegExp casa o que a original casa", () => {
    expect(copiaCasa).toBe(true);
    expect(copiaNaoCasa).toBe(false);
    expect(copiaSource).toBe("ab+c");
  });

  test("flags são herdadas da original", () => {
    expect(herdouSource).toBe("x");
    expect(herdouFlags).toBe("gi");
  });

  test("flags explícitas vencem as da original", () => {
    expect(sobrescreveSource).toBe("a");
    expect(sobrescreveFlags).toBe("g");
  });

  test("pattern vindo de variável string", () => {
    expect(deVarCasa).toBe(true);
  });

  test("string literal não regrediu", () => {
    expect(literalSource).toBe("a");
    expect(literalFlags).toBe("g");
  });

  test("flags vindas de variável", () => {
    expect(flagsDeVarFlags).toBe("gi");
  });
});
