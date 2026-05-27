import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// `new Uint8Array(arrayBuffer)` le os bytes do ArrayBuffer (snapshot).
// Combinado com DataView escrevendo no mesmo buffer + overload por aridade
// (setUint16 com littleEndian).
const buf = new ArrayBuffer(8);
const view = new DataView(buf);
view.setUint16(0, 0x1234, false); // big-endian: 12,34
view.setUint16(2, 0x5678, true);  // little-endian: 78,56
view.setInt32(4, -42, false);

print("bytes=" + Array.from(new Uint8Array(buf)).join(","));

describe("Uint8Array(ArrayBuffer) + overload aridade (57)", () => {
  test("le bytes do buffer com endianness correta", () =>
    expect(out).toBe("bytes=18,52,120,86,255,255,255,214\n"));
});
