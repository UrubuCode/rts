import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// DataView floats + flag littleEndian.
const view = new DataView(new ArrayBuffer(16));
view.setFloat64(0, 3.141592653589793, true);
view.setFloat32(8, -1.5, false);
print("f64=" + view.getFloat64(0, true).toFixed(6));
print("f32=" + view.getFloat32(8, false).toFixed(1));

// Inteiros com endianness explicita (round-trip).
const v2 = new DataView(new ArrayBuffer(8));
v2.setUint16(0, 0x1234, false);
v2.setUint16(2, 0x5678, true);
v2.setInt32(4, -42, false);
print("u16be=" + v2.getUint16(0, false).toString(16));
print("u16le=" + v2.getUint16(2, true).toString(16));
print("i32=" + v2.getInt32(4, false));

describe("DataView floats + endianness (82/57)", () => {
  test("setFloat/littleEndian round-trip", () =>
    expect(out).toBe("f64=3.141593\nf32=-1.5\nu16be=1234\nu16le=5678\ni32=-42\n"));
});
