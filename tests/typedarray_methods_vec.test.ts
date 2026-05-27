import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#93) Metodos de TypedArray (Vec backing): subarray, set([...],off),
// copyWithin, fill, includes, at(neg).
const arr = new Uint8Array([10, 20, 30, 40, 50]);
print("sub=" + Array.from(arr.subarray(1, 4)).join(","));
print("slice=" + Array.from(arr.slice(2)).join(","));
arr.set([99, 88], 1);
print("set=" + Array.from(arr).join(","));
print("at=" + arr.at(-1));
print("inc=" + arr.includes(99));

describe("typedarray methods Vec (#93)", () => {
  test("subarray/set/at(neg)/includes", () =>
    expect(out).toBe("sub=20,30,40\nslice=30,40,50\nset=10,99,88,40,50\nat=50\ninc=true\n"));
});
