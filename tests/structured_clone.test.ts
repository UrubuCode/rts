import { describe, test, expect } from "rts:test";

let out = "";

// (#375) structuredClone — deep clone independente
const obj = { a: 1, b: { x: 10 } };
const clone = structuredClone(obj);
(clone as any).a = 99;
out += (obj as any).a + "," + (clone as any).a + "\n";  // 1,99

// Array
const arr = [1, 2, 3];
const carr = structuredClone(arr);
carr.push(4);
out += arr.length + "," + carr.length + "\n";  // 3,4

describe("structured_clone", () => {
  test("deep clone independente (#375)", () => expect(out).toBe(
    "1,99\n3,4\n"
  ));
});
