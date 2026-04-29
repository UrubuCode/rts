import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// typeof on various types
print(typeof 42);
print(typeof 3.14);
print(typeof "hello");
print(typeof true);
print(typeof false);
print(typeof undefined);
print(typeof null);
print(typeof {});
print(typeof []);
print(typeof function() {});
print(typeof Symbol("x"));

// typeof on declared variables
let x: number = 5;
print(typeof x);

let s: string = "hi";
print(typeof s);

// typeof with undeclared (should be "undefined", not throw)
print(typeof undeclaredVariable);

// typeof in conditions
function checkType(v: any): string {
  if (typeof v === "number") return "number";
  if (typeof v === "string") return "string";
  if (typeof v === "boolean") return "boolean";
  return "other";
}
print(checkType(42));
print(checkType("hello"));
print(checkType(true));
print(checkType(null));

describe("typeof_operator", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "number\nnumber\nstring\nboolean\nboolean\nundefined\nobject\nobject\nobject\nfunction\nsymbol\nnumber\nstring\nundefined\nnumber\nstring\nboolean\nother\n"
  ));
});
