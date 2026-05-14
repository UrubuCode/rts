import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj = { a: 1, b: 2 };
print("a=" + obj.propertyIsEnumerable("a"));
print("b=" + obj.propertyIsEnumerable("b"));
print("c=" + obj.propertyIsEnumerable("c"));

const arr = [1, 2, 3];
print("arr_0=" + arr.propertyIsEnumerable(0));
print("arr_2=" + arr.propertyIsEnumerable(2));
print("arr_99=" + arr.propertyIsEnumerable(99));
// arrays: "length" e own mas non-enumerable
print("arr_len=" + arr.propertyIsEnumerable("length"));

describe("propertyIsEnumerable (#788)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "a=true\n" +
      "b=true\n" +
      "c=false\n" +
      "arr_0=true\n" +
      "arr_2=true\n" +
      "arr_99=false\n" +
      "arr_len=false\n"
    )
  );
});
