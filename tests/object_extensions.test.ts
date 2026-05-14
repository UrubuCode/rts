import { describe, test, expect } from "rts:test";

let out = "";

// (#771) Object.isExtensible / preventExtensions — backed por Set
// global de handles previnidos.
const obj = { x: 1 };
const ext1: boolean = Object.isExtensible(obj);
out += (ext1 ? "y" : "n") + "\n";   // y

Object.preventExtensions(obj);
const ext2: boolean = Object.isExtensible(obj);
out += (ext2 ? "y" : "n") + "\n";   // n

describe("object_extensions", () => {
  test("isExtensible/preventExtensions (#771)", () => expect(out).toBe(
    "y\nn\n"
  ));
});
