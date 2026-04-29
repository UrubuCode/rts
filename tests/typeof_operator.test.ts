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

// typeof em comparacao (feature detection)
if (typeof undeclaredVariable === "undefined") print("feat-detect-ok");
if (typeof 42 === "number") print("num-ok");
if (typeof "x" === "string") print("str-ok");

describe("typeof_operator", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "number\nnumber\nstring\nboolean\nboolean\nundefined\nobject\nobject\nobject\nfunction\nsymbol\nnumber\nstring\nundefined\nfeat-detect-ok\nnum-ok\nstr-ok\n"
  ));
});
