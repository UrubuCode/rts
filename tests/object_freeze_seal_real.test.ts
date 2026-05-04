import { describe, test, expect } from "rts:test";

// Object.freeze/seal agora persistem flag — isFrozen/isSealed retornam true.

const a = { x: 1 };
const fa = Object.isFrozen(a);
Object.freeze(a);
const fb = Object.isFrozen(a);
const fc = Object.isSealed(a); // freeze implica sealed.

const b = { y: 1 };
const sa = Object.isSealed(b);
Object.seal(b);
const sb = Object.isSealed(b);
const sc = Object.isFrozen(b); // seal nao implica frozen.

const c = { z: 1 };
const idle = Object.isFrozen(c);

describe("object_freeze_seal_real", () => {
  test("antes de freeze: nao frozen", () => expect(fa).toBe(false));
  test("apos freeze: frozen", () => expect(fb).toBe(true));
  test("freeze implica sealed", () => expect(fc).toBe(true));
  test("antes de seal: nao sealed", () => expect(sa).toBe(false));
  test("apos seal: sealed", () => expect(sb).toBe(true));
  test("seal nao implica frozen", () => expect(sc).toBe(false));
  test("objeto novo nao e frozen", () => expect(idle).toBe(false));
});
