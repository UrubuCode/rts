import { describe, test, expect } from "rts:test";

// O binding do `catch (e)` é escopado ao BLOCO do catch (spec) — não pode vazar
// e sombrear um nome externo pelo resto do corpo.
//
// O bug: `bind_catch_local` (trycatch.rs) inseria o nome em `locals` e nunca
// restaurava. Depois do try/catch, `e` continuava sendo o local Tagged do catch
// — e uma FUNÇÃO externa chamada `e` virava "TypeError: not a function" na
// primeira chamada após o bloco.
//
// Não é caso teórico: bundle minificado da Meta (bootstrap do WhatsApp Web)
// nomeia helpers de `e`/`t`/`n` e usa `catch(e){return}` na mesma função — o
// padrão exato `function e(..){..} function r(r){ try{..}catch(e){..} e(r) }`.
//
// Valores conferidos contra o Node. Pré-computado no top-level (regra do
// projeto: método dentro de test() pode perder handle pro GC).

// ── função externa `e` + catch(e) na mesma função ──────────────────────────
function e(x: any): number { return x * 2; }
function usaDepoisDoCatch(y: number): number {
  try { y = y + 1; } catch (e) { return 0; }
  return e(y); // precisa resolver a FUNÇÃO e, não o binding morto do catch
}
const depoisDoCatch = usaDepoisDoCatch(5);

// ── o catch ainda captura o erro normalmente ───────────────────────────────
let msgPega = "";
try { throw new Error("boom"); } catch (e) { msgPega = e.message; }

// ── com throw REAL no try, o caminho pós-catch também restaura ─────────────
function comThrow(): number {
  let marca = 0;
  try { throw new Error("x"); } catch (e) { marca = 1; }
  return marca + e(10); // e() de novo a função externa
}
const comThrowV = comThrow();

// ── variável externa sombreada pelo catch volta a ser visível ──────────────
function varExterna(): string {
  const s = "fora";
  try { throw new Error("dentro"); } catch (s) { /* sombra só aqui */ }
  return s;
}
const varExternaV = varExterna();

// ── catches ANINHADOS: cada nível restaura o de fora ───────────────────────
function aninhado(): string {
  let out = "";
  try {
    throw new Error("a");
  } catch (e) {
    try { throw new Error("b"); } catch (e) { out = out + e.message; }
    out = out + e.message; // o `e` de FORA voltou
  }
  return out;
}
const aninhadoV = aninhado();

// ── catch SEM binding continua funcionando ─────────────────────────────────
function semBinding(): number {
  try { throw new Error("x"); } catch { return 7; }
  return 0;
}
const semBindingV = semBinding();

describe("catch(e) é escopado ao bloco do catch", () => {
  test("função externa `e` volta a resolver depois do catch", () => {
    expect(depoisDoCatch).toBe(12);
  });

  test("o catch ainda captura o erro", () => {
    expect(msgPega).toBe("boom");
  });

  test("com throw real, o pós-catch também restaura", () => {
    expect(comThrowV).toBe(21);
  });

  test("variável externa sombreada volta", () => {
    expect(varExternaV).toBe("fora");
  });

  test("catches aninhados restauram nível a nível", () => {
    expect(aninhadoV).toBe("ba");
  });

  test("catch sem binding segue ok", () => {
    expect(semBindingV).toBe(7);
  });
});
