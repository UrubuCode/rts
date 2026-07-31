import { describe, test, expect } from "rts:test";

// O iterador de um generator LAZY tem de manter o protocolo (`next`/`return`/
// `throw`) ao atravessar QUALQUER borda dinâmica — array, objeto, argumento,
// campo de instância, `Map`, destructuring.
//
// A causa raiz (issue #2042) era de REPRESENTAÇÃO, não de despacho: o ctor de um
// generator lazy devolve o handle da `Entry::GenState` num `Repr::Int64` cru, e
// `Val::new` carimba todo `Int64` como `JsKind::Number` — então boxar o valor
// para cruzar uma borda o convertia com `fcvt_from_sint` e o handle virava um
// double comum. O `.next()` seguinte não achava a GenState e lia `undefined`,
// SILENCIOSAMENTE. O valor observado era o próprio handle: `1.39e-309` são os
// bits de `0x0001_0000_0000_0A54` (generation 1, slot 0xA54).
//
// A correção boxa o handle como word `TAG_OBJECT` já no retorno do ctor
// (`call_spread.rs`) — a forma que `try_generator_dyn`/`to_iter_array` detectam
// via `Entry::GenState` —, então a identidade sobrevive a todas as bordas de uma
// vez, em vez de exigir um remendo por borda.
//
// As não-regressões importam tanto quanto o fix: um i64 legítimo NÃO pode virar
// object word, e um array comum NÃO pode passar a responder a `.next()`.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function* gl() { let i = 1; while (i <= 3) { yield i; i = i + 1; } }

// ── as bordas ──────────────────────────────────────────────────────────────
const viaArray = [gl()][0].next().value;

const obj = { it: gl() };
const viaObjeto = obj.it.next().value;

function consome(x) { return x.next(); }
const viaArgumento = consome(gl()).value;

const empurrado: any[] = [];
empurrado.push(gl());
const viaPush = empurrado[0].next().value;

class Portador { it: any; constructor() { this.it = gl(); } }
const viaCampo = new Portador().it.next().value;

const mapa = new Map();
mapa.set("k", gl());
const viaMap = mapa.get("k").next().value;

const [desestruturado] = [gl()];
const viaDestructuring = desestruturado.next().value;

// atravessa a borda E consome em sequência (o cursor tem de ser o MESMO)
const guardado = { it: gl() };
const sequencia =
  guardado.it.next().value + guardado.it.next().value + guardado.it.next().value;

// ── não-regressões ─────────────────────────────────────────────────────────
const direto = gl().next().value;
const inteiroEmArray = [42][0];
const grandeEmArray = [9007199254740991][0];
const arrayComumNext = ([1, 2] as any).next;
const porSpread = [...gl()].join(",");
let somaForOf = 0;
for (const x of gl()) { somaForOf = somaForOf + x; }

describe("iterador de generator atravessa bordas dinâmicas", () => {
  test("elemento de array literal", () => {
    expect(viaArray).toBe(1);
  });

  test("campo de objeto literal", () => {
    expect(viaObjeto).toBe(1);
  });

  test("argumento de função", () => {
    expect(viaArgumento).toBe(1);
  });

  test("push em array", () => {
    expect(viaPush).toBe(1);
  });

  test("campo de instância de classe", () => {
    expect(viaCampo).toBe(1);
  });

  test("valor guardado em Map", () => {
    expect(viaMap).toBe(1);
  });

  test("destructuring de array", () => {
    expect(viaDestructuring).toBe(1);
  });

  test("cursor avança na travessia (mesmo iterador, não uma cópia)", () => {
    expect(sequencia).toBe(6);
  });
});

describe("não-regressões do value model", () => {
  test("chamada direta não regrediu", () => {
    expect(direto).toBe(1);
  });

  test("i64 legítimo em array continua número", () => {
    expect(inteiroEmArray).toBe(42);
  });

  test("i64 grande em array preserva o valor exato", () => {
    expect(grandeEmArray).toBe(9007199254740991);
  });

  test("array comum não responde a .next", () => {
    expect(arrayComumNext).toBe(undefined);
  });

  test("spread de generator não regrediu", () => {
    expect(porSpread).toBe("1,2,3");
  });

  test("for-of de generator não regrediu", () => {
    expect(somaForOf).toBe(6);
  });
});
