import { describe, test, expect } from "rts:test";

// `RegExp` chamado SEM `new` e `RegExp` lido como VALOR de primeira classe.
//
// A spec faz `RegExp(p, f)` equivalente a `new RegExp(p, f)`, com UMA diferença:
// se o argumento JÁ é uma RegExp e `flags` é `undefined`, a chamada SEM `new`
// devolve o MESMO objeto (identidade); com `new`, copia.
//
// Antes: `RegExp("ab+c")` era `call to unknown function \`RegExp\`` (bail de
// compilação) e `const f = RegExp` era `ReferenceError: RegExp is not defined`.
//
// Valores conferidos contra o Node. Pré-computado no top-level (chamar método de
// instância dentro do closure de `test()` pode esbarrar no GC).

// ── sem `new` ───────────────────────────────────────────────────────────────
const semNew = RegExp("ab+c");
const semNewTest = semNew.test("abbbc");
const semNewTestNao = semNew.test("xyz");
const semNewSource = semNew.source;
const semNewFlags = semNew.flags;

// ── sem `new`, com flags ────────────────────────────────────────────────────
const comFlags = RegExp("a.c", "gi");
const comFlagsSource = comFlags.source;
const comFlagsFlags = comFlags.flags;
const comFlagsGlobal = comFlags.global;
const comFlagsIgnore = comFlags.ignoreCase;
const comFlagsTest = comFlags.test("AxC");

// ── com `new` (não pode regredir) ───────────────────────────────────────────
const comNew = new RegExp("ab+c");
const comNewTest = comNew.test("abbbc");
const comNewSource = comNew.source;
const comNewFlagsVazio = comNew.flags;
const comNewFlags = new RegExp("a.c", "gi");
const comNewFlagsFlags = comNewFlags.flags;

// ── literal `/re/` (não pode regredir) ──────────────────────────────────────
const lit = /ab+c/;
const litTest = lit.test("abbbc");
const litSource = lit.source;
const litG = /a.c/gi;
const litGFlags = litG.flags;
const litReplace = "xAxCx".replace(/a.c/gi, "-");
const litSplit = "a1b2c".split(/\d/).join("|");
const litMatch = ("foo123bar".match(/\d+/) || ["?"])[0];

// ── identidade: RegExp(re) sem flags devolve o MESMO objeto ─────────────────
const orig = /ab+c/g;
const mesmo = RegExp(orig);
const ehMesmo = mesmo === orig;
const mesmoFlags = mesmo.flags;

// ── cópia: RegExp(re, "novas flags") cria outro objeto ──────────────────────
const copia = RegExp(orig, "i");
const copiaEhOutro = copia === orig;
const copiaSource = copia.source;
const copiaFlags = copia.flags;
// A cópia precisa casar o que a ORIGINAL casa — copiar via ToString(re) daria
// "/ab+c/g" COM as barras e a regex deixaria de casar.
const copiaTest = copia.test("ABBBC");

// ── `new RegExp(re)` também copia pelo source, não pelo ToString ────────────
const copiaNew = new RegExp(orig);
const copiaNewTest = copiaNew.test("abbbc");
const copiaNewSource = copiaNew.source;

// ── RegExp como VALOR de primeira classe ────────────────────────────────────
const F = RegExp;
const porValor = F("ab+c");
const porValorTest = porValor.test("abbbc");
const porValorSource = porValor.source;
const porValorTipo = typeof F;

const porValorNew = new F("a.c", "i");
const porValorNewTest = porValorNew.test("AxC");
const porValorNewFlags = porValorNew.flags;

// ── RegExp passado a uma função e chamado lá dentro ─────────────────────────
function constroi(ctor: any, pat: string, flags: string) {
  return ctor(pat, flags);
}
const viaParam = constroi(RegExp, "z+", "i");
const viaParamTest = viaParam.test("ZZZ");
const viaParamSource = viaParam.source;
const viaParamFlags = viaParam.flags;

// ── exec nos objetos produzidos por cada caminho ────────────────────────────
const execSemNew = (RegExp("(\\d+)").exec("abc42def") || ["?"])[0];
const execComNew = (new RegExp("(\\d+)").exec("abc42def") || ["?"])[0];
const execLit = (/(\d+)/.exec("abc42def") || ["?"])[0];
const execValor = (F("(\\d+)").exec("abc42def") || ["?"])[0];

// ── métodos de string com uma RegExp construída sem `new` ───────────────────
const strReplace = "xAxCx".replace(RegExp("a.c", "gi"), "-");
const strSearch = "foo123".search(RegExp("\\d"));

describe("RegExp sem `new` e como valor", () => {
  test("RegExp(pat) sem `new` constrói e casa", () => {
    expect(semNewTest).toBe(true);
    expect(semNewTestNao).toBe(false);
    expect(semNewSource).toBe("ab+c");
    expect(semNewFlags).toBe("");
  });

  test("RegExp(pat, flags) sem `new` preserva as flags", () => {
    expect(comFlagsSource).toBe("a.c");
    expect(comFlagsFlags).toBe("gi");
    expect(comFlagsGlobal).toBe(true);
    expect(comFlagsIgnore).toBe(true);
    expect(comFlagsTest).toBe(true);
  });

  test("`new RegExp(...)` continua igual", () => {
    expect(comNewTest).toBe(true);
    expect(comNewSource).toBe("ab+c");
    expect(comNewFlagsVazio).toBe("");
    expect(comNewFlagsFlags).toBe("gi");
  });

  test("literal /re/ continua intacto", () => {
    expect(litTest).toBe(true);
    expect(litSource).toBe("ab+c");
    expect(litGFlags).toBe("gi");
    expect(litReplace).toBe("x-x");
    expect(litSplit).toBe("a|b|c");
    expect(litMatch).toBe("123");
  });

  test("RegExp(re) sem flags devolve o MESMO objeto (identidade da spec)", () => {
    expect(ehMesmo).toBe(true);
    expect(mesmoFlags).toBe("g");
  });

  test("RegExp(re, flags) copia com as flags novas e continua casando", () => {
    expect(copiaEhOutro).toBe(false);
    expect(copiaSource).toBe("ab+c");
    expect(copiaFlags).toBe("i");
    expect(copiaTest).toBe(true);
  });

  test("`new RegExp(re)` copia pelo source, não pelo ToString", () => {
    expect(copiaNewSource).toBe("ab+c");
    expect(copiaNewTest).toBe(true);
  });

  test("RegExp lido como valor é chamável", () => {
    expect(porValorTipo).toBe("function");
    expect(porValorTest).toBe(true);
    expect(porValorSource).toBe("ab+c");
  });

  test("`new` sobre o valor RegExp constrói", () => {
    expect(porValorNewTest).toBe(true);
    expect(porValorNewFlags).toBe("i");
  });

  test("RegExp passado a uma função e chamado lá dentro", () => {
    expect(viaParamTest).toBe(true);
    expect(viaParamSource).toBe("z+");
    expect(viaParamFlags).toBe("i");
  });

  test("exec funciona nos objetos de todos os caminhos", () => {
    expect(execSemNew).toBe("42");
    expect(execComNew).toBe("42");
    expect(execLit).toBe("42");
    expect(execValor).toBe("42");
  });

  test("métodos de string aceitam uma RegExp construída sem `new`", () => {
    expect(strReplace).toBe("x-x");
    expect(strSearch).toBe(3);
  });
});
