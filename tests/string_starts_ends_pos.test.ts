import { describe, test, expect } from "rts:test";

let out = "";

const s = "hello world";

// (#208) startsWith(prefix, position?)
const r1: boolean = s.startsWith("world", 6);
const r2: boolean = s.startsWith("hello");
const r3: boolean = s.startsWith("xyz", 0);
out += (r1 ? "y" : "n") + (r2 ? "y" : "n") + (r3 ? "y" : "n") + "\n";

// (#208) endsWith(suffix, endPosition?)
const e1: boolean = s.endsWith("hello", 5);
const e2: boolean = s.endsWith("world");
const e3: boolean = s.endsWith("xyz", 11);
out += (e1 ? "y" : "n") + (e2 ? "y" : "n") + (e3 ? "y" : "n") + "\n";

describe("string_starts_ends_pos", () => {
  test("starts/endsWith com offset (#208)", () => expect(out).toBe(
    "yyn\nyyn\n"
  ));
});
