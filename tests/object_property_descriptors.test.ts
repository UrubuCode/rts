import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj = { a: 1, b: 2 };
const descs = Object.getOwnPropertyDescriptors(obj);
print("keys=" + Object.keys(descs).sort().join(","));
print("a.value=" + descs.a.value);

const desc = Object.getOwnPropertyDescriptor(obj, "a");
print("a.value2=" + desc.value);

describe("Object.getOwnPropertyDescriptor(s) (#749)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "keys=a,b\n" +
      "a.value=1\n" +
      "a.value2=1\n"
    )
  );
});
