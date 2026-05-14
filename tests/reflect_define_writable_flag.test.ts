import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj: any = {};

// defineProperty com writable:false e enumerable:true (Bun/Node JS spec)
const success = Reflect.defineProperty(obj, "a", {
  value: 42,
  writable: false,
  enumerable: true,
});
print("success=" + success);
print("a=" + obj.a);

const desc = Reflect.getOwnPropertyDescriptor(obj, "a");
print("value=" + Reflect.get(desc, "value"));
print("writable=" + Reflect.get(desc, "writable"));
print("enumerable=" + Reflect.get(desc, "enumerable"));

// defineProperty com defaults (writable:true, enumerable:true)
const obj2: any = {};
Reflect.defineProperty(obj2, "x", { value: 1, writable: true, enumerable: true });
const desc2 = Reflect.getOwnPropertyDescriptor(obj2, "x");
print("w2=" + Reflect.get(desc2, "writable"));

describe("Reflect.defineProperty preserves writable/enumerable (#795)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "success=true\n" +
      "a=42\n" +
      "value=42\n" +
      "writable=false\n" +
      "enumerable=true\n" +
      "w2=true\n"
    )
  );
});
