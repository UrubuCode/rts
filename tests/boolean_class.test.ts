import { describe, test, expect } from "rts:test";

let out = "";

// Boolean() coercao
out += (Boolean(0) ? "t" : "f") + "\n";    // f
out += (Boolean(1) ? "t" : "f") + "\n";    // t
out += (Boolean(42) ? "t" : "f") + "\n";   // t
out += (Boolean(-1) ? "t" : "f") + "\n";   // t
// Strings em RTS sao handles != 0 mesmo quando vazias — Boolean("") sera true (limitacao documentada).
out += (Boolean("hello") ? "t" : "f") + "\n"; // t

// toString via const (literal pode nao casar com member dispatch)
const bt = true;
const bf = false;
out += bt.toString() + "\n";   // true
out += bf.toString() + "\n";   // false

// valueOf
out += (bt.valueOf() ? "t" : "f") + "\n"; // t
out += (bf.valueOf() ? "t" : "f") + "\n"; // f

describe("boolean_class", () => {
  test("coerce + toString + valueOf", () => expect(out).toBe(
    "f\nt\nt\nt\nt\ntrue\nfalse\nt\nf\n"
  ));
});
