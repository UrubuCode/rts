import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj = { a: 1, b: 2 };
print("obj=" + Object.getOwnPropertyNames(obj).sort().join(","));

const arr = [1, 2, 3];
print("arr=" + Object.getOwnPropertyNames(arr).sort().join(","));

const empty = {};
print("empty=" + Object.getOwnPropertyNames(empty).join(","));

describe("Object.getOwnPropertyNames (#789)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "obj=a,b\n" +
      "arr=0,1,2,length\n" +
      "empty=\n"
    )
  );
});
