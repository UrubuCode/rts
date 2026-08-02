import { describe, test, expect } from "rts:test";

// Escape analysis (RTS_OPTIMIZATION.md §5 Tier 4.1) troca um `new C(...)` que
// PROVADAMENTE não escapa por uma Cranelift `Variable` por campo — sem alocação,
// sem slot-0 de shape, sem site de IC. Estes testes fixam a FRONTEIRA: cada caso
// que escapa precisa continuar com o objeto real, e cada caso que não escapa
// precisa dar exatamente o mesmo resultado que daria com a alocação.
//
// `RTS_ESCAPE=0` deve imprimir o mesmo para todos eles — a análise não é
// semântica, é só representação.

class Pt { x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
}

// --- casos que NÃO escapam: resultado tem de bater com a versão alocada ---
function sumFields(n: number): number {
  let s = 0;
  for (let i = 0; i < n; i++) { const p = new Pt(i, i + 1); s += p.x + p.y; }
  return s;
}
function ctorComputesFields(): number {
  const p = new Pt(2 * 3, 10 - 4);
  return p.x + p.y;
}
function fractionalFields(): number {
  const p = new Pt(0.5, 0.25);
  return p.x + p.y;
}
function negativeAndZero(): number {
  const p = new Pt(-0, 0);
  return 1 / (p.x + p.y);   // -0 + 0 === +0 => +Infinity
}

// --- casos que ESCAPAM: precisam continuar sendo objetos de verdade ---
function returned(): Pt { return new Pt(1, 2); }
function stored(): number { const a: Pt[] = []; const p = new Pt(3, 4); a.push(p); return a[0].x + a[0].y; }
function identity(): boolean { const p = new Pt(1, 2); const q = p; return p === q; }
function passedToCall(): number { const p = new Pt(5, 6); return take(p); }
function take(p: Pt): number { return p.x * p.y; }
function capturedByArrow(): number { const p = new Pt(7, 8); const f = () => p.x + p.y; return f(); }
function fieldWritten(): number { const p = new Pt(1, 1); p.x = 9; return p.x + p.y; }
function usedAsKey(): number { const p = new Pt(1, 2); const m = new Map<Pt, number>(); m.set(p, 42); return m.get(p) as number; }

const r = returned();

describe("escape analysis preserves semantics", () => {
  test("field sum over a non-escaping local", () => {
    expect(sumFields(4)).toBe(16);      // (0+1)+(1+2)+(2+3)+(3+4)
  });
  test("constructor argument expressions are evaluated", () => {
    expect(ctorComputesFields()).toBe(12);
  });
  test("fractional fields keep their fraction", () => {
    expect(fractionalFields()).toBe(0.75);
  });
  test("-0 and 0 fields behave as JS says", () => {
    expect(negativeAndZero()).toBe(Infinity);
  });

  test("a returned instance is a real object", () => {
    expect(r.x + r.y).toBe(3);
    expect(r instanceof Pt).toBe(true);
  });
  test("an instance stored into an array survives", () => {
    expect(stored()).toBe(7);
  });
  test("identity is preserved when the local is aliased", () => {
    expect(identity()).toBe(true);
  });
  test("an instance passed to a call is a real object", () => {
    expect(passedToCall()).toBe(30);
  });
  test("an instance captured by an arrow is a real object", () => {
    expect(capturedByArrow()).toBe(15);
  });
  test("a written field still works", () => {
    expect(fieldWritten()).toBe(10);
  });
  test("an instance used as a Map key keeps its identity", () => {
    expect(usedAsKey()).toBe(42);
  });
});
