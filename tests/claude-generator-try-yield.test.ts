import { describe, test, expect } from "rts:test";

// `try { ...yield... } catch (e) { ...sem yield... }` — a forma COMUM — era
// inelegível para a state-machine e caía no eager-buffer, onde `yield` de VALOR
// vira `push(...)`. O caso simplesmente não compilava
// ("expression raw/unrecognized: Yield").
//
// O caminho já existia e estava certo (ENTER_TRY_CATCH → body → CAUGHT → catch),
// mas a condição de entrada exigia yield NO CATCH. Basta o TRY suspender: o
// catch sem yield é lowerado como statements ordinários dentro do estado do
// catch — nada mais é preciso.
//
// LIMITE mantido: `try/catch` combinado com `finally` segue fora desta fatia.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function* catchVazio() { try { const v = yield 1; yield v; } catch (e) { } }
const ia = catchVazio();
const vazioPrimeiro = ia.next().value;
const vazioEnviado = ia.next(5).value;

function* catchComYield() { try { const v = yield 1; yield v; } catch (e) { yield 9; } }
const ib = catchComYield();
const comYieldPrimeiro = ib.next().value;
const comYieldEnviado = ib.next(5).value;

function* continuaDepois() { try { const v = yield 1; yield v * 2; } catch (e) { } yield 99; }
const id = continuaDepois();
const d1 = id.next().value;
const d2 = id.next(3).value;
const d3 = id.next().value;

function* catchComCorpo() {
  let marca = 0;
  try { const v = yield 1; yield v; } catch (e) { marca = 1; }
  yield marca;
}
const ie = catchComCorpo();
ie.next();
ie.next(7);
const marcaFinal = ie.next().value;

// ── não-regressões ─────────────────────────────────────────────────────────
function* tryFinally() { try { yield 1; } finally { } yield 2; }
const comFinally = [...tryFinally()].join(",");

function* semTry() { const a = yield 1; yield a * 3; }
const st = semTry();
const semTryPrimeiro = st.next().value;
const semTryEnviado = st.next(4).value;

describe("try com yield no BODY e catch sem yield", () => {
  test("catch vazio: primeiro valor", () => expect(vazioPrimeiro).toBe(1));
  test("catch vazio: valor ENVIADO chega", () => expect(vazioEnviado).toBe(5));
  test("catch COM yield continua funcionando", () => expect(comYieldEnviado).toBe(5));
  test("execução continua depois do try", () =>
    expect(d1 + "," + d2 + "," + d3).toBe("1,6,99"));
  test("catch com corpo (sem yield) não é executado sem erro", () =>
    expect(marcaFinal).toBe(0));
});

describe("não-regressões", () => {
  test("try/finally não regrediu", () => expect(comFinally).toBe("1,2"));
  test("generator sem try: primeiro", () => expect(semTryPrimeiro).toBe(1));
  test("generator sem try: enviado", () => expect(semTryEnviado).toBe(12));
});
