import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#811/205) TypedArray como VIEW VIVA sobre ArrayBuffer: views de larguras
// diferentes sobre o mesmo buffer compartilham as escritas.
const buffer = new ArrayBuffer(8);
const view8 = new Uint8Array(buffer);
view8[0] = 255;
view8[1] = 128;
print("v8_0=" + view8[0]);
print("v8_1=" + view8[1]);

const view32 = new Uint32Array(buffer);
print("v32_0=" + view32[0]); // 255 + 128*256 = 33023 (little-endian)

// Escrita via view32 reflete em view8.
const buf2 = new ArrayBuffer(4);
const i32 = new Int32Array(buf2);
i32[0] = -1;
const u8 = new Uint8Array(buf2);
print("u8_0=" + u8[0]); // 255 (byte baixo de -1)

describe("TypedArray view compartilhada (#811/205)", () => {
  test("views de larguras diferentes compartilham o buffer", () =>
    expect(out).toBe("v8_0=255\nv8_1=128\nv32_0=33023\nu8_0=255\n"));
});
