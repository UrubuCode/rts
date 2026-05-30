import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#68 parcial) `view.set(srcArray, offset?)` onde `view` eh uma
// TypedArray-view nomeada sobre (Shared)ArrayBuffer escreve os elementos
// no buffer subjacente. Antes caia em VEC_SET_FROM (so' Entry::Vec) e era
// no-op silencioso sobre Buffer. Agora roteia para TA_SET_FROM.
const ab = new ArrayBuffer(16);

const i32 = new Int32Array(ab);
i32.set([100, 200, 300]);
const i32Read = i32[0] + "," + i32[1] + "," + i32[2];

const u8 = new Uint8Array(ab);
u8.set([5, 6, 7], 1); // offset 1
const u8Read = u8[0] + "," + u8[1] + "," + u8[2] + "," + u8[3];

// SharedArrayBuffer tambem (backing identico).
const sab = new SharedArrayBuffer(8);
const sv = new Int32Array(sab);
sv.set([42, 99]);
const svRead = sv[0] + "," + sv[1];

describe("typed_array_view_set (#68 parcial)", () => {
  test("Int32Array.set escreve no buffer", () => expect(i32Read).toBe("100,200,300"));
  test("Uint8Array.set com offset", () => expect(u8Read).toBe("100,5,6,7"));
  test("SharedArrayBuffer view.set", () => expect(svRead).toBe("42,99"));
});
