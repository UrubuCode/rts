import { describe, test, expect } from "rts:test";

// (#394) structuredClone preserva o TIPO do valor clonado. Antes o clone
// de Date/Map/Set/RegExp chegava sem local_class_ty no receptor -> metodos
// (.getUTCFullYear/.has/.size) mis-despachavam (SIGILL). Agora decls.rs
// propaga o tipo do arg ident para o var receptor do structuredClone.
const d = new Date("2026-06-15T00:00:00Z");
const dc = structuredClone(d);
const dcYear = dc.getUTCFullYear();      // 2026
const dcIsDate = dc instanceof Date;     // true

const m = new Map<string, number>([["a", 1], ["b", 2]]);
const mc = structuredClone(m);
const mcGet = mc.get("a");               // 1
const mcSize = mc.size;                  // 2

const s = new Set<number>([10, 20]);
const sc = structuredClone(s);
const scHas = sc.has(20);                // true
const scSize = sc.size;                  // 2

describe("structuredclone_type_preserve (#394)", () => {
  test("Date clone getUTCFullYear", () => expect(dcYear).toBe(2026));
  test("Date clone instanceof", () => expect(dcIsDate).toBe(true));
  test("Map clone get", () => expect(mcGet).toBe(1));
  test("Map clone size", () => expect(mcSize).toBe(2));
  test("Set clone has", () => expect(scHas).toBe(true));
  test("Set clone size", () => expect(scSize).toBe(2));
});
