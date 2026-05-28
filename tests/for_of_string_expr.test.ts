import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const ch of <expr>)` onde expr produz string
// nao-literal/var (fn ret string, metodo ret string, concat a+b) nao iterava.
// for_of_iterates_string so' reconhecia literal/Tpl/Ident(local_string_vars).
// Fix: estende p/ Bin Add (concat) + Call de fn/metodo em FNS_RET_STRING.

let out = "";
function print(v: string): void { out += v + "\n"; }

function getStr(): string { return "abc"; }
let r1 = "";
for (const ch of getStr()) r1 += ch + "-";
print(r1);                       // a-b-c-

class Box { val(): string { return "xyz"; } }
const b = new Box();
let r2 = "";
for (const ch of b.val()) r2 += ch;
print(r2);                       // xyz

// concat a + b
let r3 = "";
for (const ch of ("a" + "b" + "c")) r3 += ch;
print(r3);                       // abc

// concat com var string
const prefix = "X";
let r4 = "";
for (const ch of (prefix + "yz")) r4 += ch;
print(r4);                       // Xyz

// guard: literal e var local continuam OK
let r5 = "";
for (const ch of "12") r5 += ch + ".";
print(r5);                       // 1.2.

describe("for-of string expr", () => {
  test("for-of sobre fn/metodo/concat string itera chars", () =>
    expect(out).toBe("a-b-c-\nxyz\nabc\nXyz\n1.2.\n"));
});
