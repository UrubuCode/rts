import { describe, test, expect } from "rts:test";

let out = "";

// `matchAll` answers an ITERATOR (a "RegExp String Iterator"), not an array —
// confirmed against Node v20: `typeof s.matchAll(re)` is `"object"`,
// `Array.isArray(...)` is `false`, and there is no `.length`/`.join` on it.
// This file used to call `.join` straight on the result and pinned an eager
// array as correct; that was this engine's OLD `matchAll`, since fixed (see
// `crates/rts-core/src/entry/string/pattern.rs::iterator_over`) to answer a
// real iterator like every other engine. The fix here is the test, per the
// same Node measurement: spread into an array first, the way any program
// driving `matchAll` for its matches (rather than by hand, one `.next()` at
// a time) has to.
const s = "ana banana";
const matches = [...s.matchAll("an")];
out += matches.length + "\n";   // 3
out += matches.map((m) => m[0]).join(",") + "\n";  // an,an,an

// Sem matches — array vazio
const empty = [...("xyz".matchAll("an"))];
out += empty.length + "\n";    // 0

// Pattern com classe
const s2 = "Hello World";
const ms = [...s2.matchAll("[Hh]")];
out += ms.length + "\n";       // 1 (so' "H")
out += ms.map((m) => m[0]).join(",") + "\n";    // H

describe("string_match_all", () => {
  test("matchAll retorna um ITERADOR (#208) — [...matches] antes de .join", () => expect(out).toBe(
    "3\nan,an,an\n0\n1\nH\n"
  ));
});
