import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// ArrayBuffer.prototype.slice(start, end) -> novo ArrayBuffer.
const buffer = new ArrayBuffer(8);
const view = new DataView(buffer);
view.setUint8(0, 10);
view.setUint8(1, 20);
view.setUint8(4, 99);

const a = buffer.slice(0, 4);
print("a_len=" + a.byteLength);
const av = new DataView(a);
print("a0=" + av.getUint8(0));
print("a1=" + av.getUint8(1));

const b = buffer.slice(4, 8);
print("b_len=" + b.byteLength);
print("b0=" + new DataView(b).getUint8(0));

describe("ArrayBuffer.slice", () => {
  test("slice copia o range de bytes", () =>
    expect(out).toBe("a_len=4\na0=10\na1=20\nb_len=4\nb0=99\n"));
});
