import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// ArrayBuffer + DataView (big-endian por padrao, igual JS).
const buffer = new ArrayBuffer(8);
const view = new DataView(buffer);

view.setUint8(0, 255);
view.setUint8(1, 128);
print("get0=" + view.getUint8(0));
print("get1=" + view.getUint8(1));

view.setUint16(2, 1000);
print("get16=" + view.getUint16(2));

view.setInt32(4, -123456);
print("get32=" + view.getInt32(4));

print("byteLength=" + view.byteLength);
print("byteOffset=" + view.byteOffset);

describe("ArrayBuffer + DataView (206)", () => {
  test("set/get u8/u16/i32 big-endian", () =>
    expect(out).toBe(
      "get0=255\nget1=128\nget16=1000\nget32=-123456\nbyteLength=8\nbyteOffset=0\n"
    ));
});
