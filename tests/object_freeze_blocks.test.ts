import { describe, test, expect } from "rts:test";

// Object.freeze agora bloqueia mutacoes silenciosamente (JS non-strict).
// Object.seal permite update de props existentes mas nao add de novas.

const f = Object.freeze({ n: 42 } as { n: number; m?: number });
f.n = 99;          // ignorado
f.m = 1;           // ignorado
const r1 = f.n;    // 42

const s = Object.seal({ x: 1 } as { x: number; y?: number });
s.x = 100;         // permitido (existing key)
s.y = 200;         // ignorado (new key)
const r2 = s.x;    // 100
const r3 = s.y;    // undefined-ish (sem field)

describe("object_freeze_blocks", () => {
  test("freeze bloqueia mutacao em key existente", () => expect(r1).toBe(42));
  test("seal permite update", () => expect(r2).toBe(100));
});
