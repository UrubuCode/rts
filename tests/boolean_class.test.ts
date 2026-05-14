import { describe, test, expect } from "rts:test";

let out = "";

// Boolean() coercao
out += (Boolean(0) ? "t" : "f") + "\n";    // f
out += (Boolean(1) ? "t" : "f") + "\n";    // t
out += (Boolean(42) ? "t" : "f") + "\n";   // t
out += (Boolean(-1) ? "t" : "f") + "\n";   // t
out += (Boolean("hello") ? "t" : "f") + "\n"; // t
out += (Boolean("") ? "t" : "f") + "\n";   // f (#879)
out += (Boolean(null) ? "t" : "f") + "\n"; // f (#879)
out += (Boolean(undefined) ? "t" : "f") + "\n"; // f (#879)

// toString via const (literal pode nao casar com member dispatch)
const bt = true;
const bf = false;
out += bt.toString() + "\n";   // true
out += bf.toString() + "\n";   // false

// valueOf
out += (bt.valueOf() ? "t" : "f") + "\n"; // t
out += (bf.valueOf() ? "t" : "f") + "\n"; // f

// new Boolean(x) — boxed (#879)
let boxed = "";
const b1 = new Boolean(true);
const b2 = new Boolean(false);
boxed += "valueOf_true=" + b1.valueOf() + "\n";
boxed += "valueOf_false=" + b2.valueOf() + "\n";
boxed += "toString_true=" + b1.toString() + "\n";
boxed += "toString_false=" + b2.toString() + "\n";
boxed += "typeof=" + (typeof b1) + "\n";

describe("boolean_class", () => {
  test("coerce + toString + valueOf", () => expect(out).toBe(
    "f\nt\nt\nt\nt\nf\nf\nf\ntrue\nfalse\nt\nf\n"
  ));
  test("new Boolean boxed (#879)", () => expect(boxed).toBe(
    "valueOf_true=true\nvalueOf_false=false\ntoString_true=true\ntoString_false=false\ntypeof=object\n"
  ));
});
