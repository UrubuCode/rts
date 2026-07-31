import { describe, test, expect } from "rts:test";

// Construtor de classe do Registry cujo parâmetro é string (`new URL(x)`,
// `new TextDecoder(x)`, …) alimentado com algo que NÃO é string literal.
//
// Antes bailava — "argument 0 is not a proven string" — por receio de que o
// marshal coagisse por baixo dos panos. Mas a spec desses construtores MANDA
// `ToString(x)`, então coagir DE PROPÓSITO, no lugar visível, é a semântica, não
// uma mudança de comportamento. O bail custava scripts reais de página, onde a
// URL quase nunca é literal (vem de concatenação, de variável, de campo).
//
// Valores conferidos contra o Node. Pré-computado no top-level.

// ── concatenação ───────────────────────────────────────────────────────────
const base = "https://exemplo.com";
const porConcat = new URL(base + "/caminho");
const concatPath = porConcat.pathname;
const concatHost = porConcat.hostname;

// ── variável simples ───────────────────────────────────────────────────────
const texto = "https://outro.org/a/b?q=1";
const porVar = new URL(texto);
const varPath = porVar.pathname;
const varQuery = porVar.search;

// ── campo de objeto ────────────────────────────────────────────────────────
const cfg: any = { endpoint: "https://api.dev/v1" };
const porCampo = new URL(cfg.endpoint);
const campoPath = porCampo.pathname;

// ── retorno de função ──────────────────────────────────────────────────────
function monta(): string { return "https://fn.io/z"; }
const porFn = new URL(monta());
const fnPath = porFn.pathname;

// ── literal continua funcionando (não pode regredir) ───────────────────────
const literal = new URL("https://lit.com/x");
const litPath = literal.pathname;

describe("construtor com parâmetro string coage o argumento", () => {
  test("concatenação", () => {
    expect(concatPath).toBe("/caminho");
    expect(concatHost).toBe("exemplo.com");
  });

  test("variável", () => {
    expect(varPath).toBe("/a/b");
    expect(varQuery).toBe("?q=1");
  });

  test("campo de objeto", () => {
    expect(campoPath).toBe("/v1");
  });

  test("retorno de função", () => {
    expect(fnPath).toBe("/z");
  });

  test("literal não regrediu", () => {
    expect(litPath).toBe("/x");
  });
});
