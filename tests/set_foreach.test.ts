import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const s = new Set([1, 2, 3]);

const collected: string[] = [];
s.forEach((value: number, value2: number) => {
  collected.push(value + ":" + (value === value2));
});

print("size=" + s.size);
print("forEach=" + collected.join(","));

describe("Set.forEach((value, value2, set)) (#793 follow-up)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "size=3\n" +
      "forEach=1:true,2:true,3:true\n"
    )
  );
});
