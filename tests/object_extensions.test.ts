import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Object.isExtensible / preventExtensions — v0 stubs.
const obj = { x: 1 };
const ext1: boolean = Object.isExtensible(obj);
out += (ext1 ? "y" : "n") + "\n";   // y

Object.preventExtensions(obj);
// v0: ainda retorna true (sem flag de extensible no Entry).
const ext2: boolean = Object.isExtensible(obj);
out += (ext2 ? "y" : "n") + "\n";   // y (v0 limit)

describe("object_extensions", () => {
  test("isExtensible/preventExtensions v0 (#208)", () => expect(out).toBe(
    "y\ny\n"
  ));
});
