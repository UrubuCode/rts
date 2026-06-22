import { describe, test, expect } from "rts:test";

// `new ArrayBuffer(n)` + `new DataView(buf)` via Registry-class instantiation,
// com roundtrip dos acessores (method calls pelo caminho registry). Resultados
// devem casar com bun/node (big-endian por padrão).

const buf = new ArrayBuffer(8);
const dv = new DataView(buf);
dv.setUint8(0, 65);
dv.setUint8(1, 200);
dv.setUint16(2, 4660);
dv.setInt32(4, 305419896);

const a = dv.getUint8(0);
const b = dv.getUint8(1);
const c = dv.getUint16(2);
const d = dv.getInt32(4);

describe("fixture:arraybuffer_dataview", () => {
  test("setUint8/getUint8 roundtrip", () => {
    expect(a).toBe(65);
  });
  test("getUint8 preserves high byte", () => {
    expect(b).toBe(200);
  });
  test("setUint16/getUint16 roundtrip (big-endian)", () => {
    expect(c).toBe(4660);
  });
  test("setInt32/getInt32 roundtrip (big-endian)", () => {
    expect(d).toBe(305419896);
  });
});
