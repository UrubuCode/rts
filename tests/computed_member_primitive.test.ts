// Computed (runtime-named) member access on a PRIMITIVE receiver.
//
// `s[k]` / `s[k]()` where `k` is only known at runtime must reach the same
// primitive method surface a statically-named `s.toUpperCase()` reaches — via
// the intrinsic (autoboxed) `String`/`Number`/`Boolean` prototype. Object and
// user-class receivers already worked; they are kept here as regression guards.
//
// Every key below is BUILT at runtime (string concat), so nothing can be
// resolved at compile time — the dispatch is genuinely by runtime name.

import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const s = "abc";

// --- string method, no args ---
const kUpper = "to" + "UpperCase";
print("upper=" + (s as any)[kUpper]());

const kTrim = "tr" + "im";
print("trim=[" + ("  pad  " as any)[kTrim]() + "]");

// --- string method WITH args ---
const kSlice = "sli" + "ce";
print("slice=" + (s as any)[kSlice](1, 2));

const kIndexOf = "index" + "Of";
print("indexOf=" + ("hello" as any)[kIndexOf]("ll"));

// --- computed property READ on a primitive (not a call) ---
const kLen = "len" + "gth";
print("length=" + (s as any)[kLen]);
print("methodIsFn=" + (typeof (s as any)[kUpper]));

// --- numeric index still reads the code unit (not shadowed by the name path) ---
print("idx0=" + (s as any)[0]);
print("idxStr1=" + (s as any)["1"]);
print("idxOOR=" + (s as any)[99]);

// --- number receiver ---
const n = 255;
const kToString = "to" + "String";
print("radix=" + (n as any)[kToString](16));
const kFixed = "to" + "Fixed";
print("fixed=" + (1.5 as any)[kFixed](2));

// --- boolean receiver ---
const b = true;
print("bool=" + (b as any)[kToString]());

// --- a name that does NOT exist must throw TypeError (like Node) ---
let threw = "no";
try {
  const kMissing = "no" + "SuchMethod";
  (s as any)[kMissing]();
} catch (e: any) {
  threw = "TypeError";
}
print("missing=" + threw);
print("missingRead=" + (s as any)["no" + "SuchProp"]);

// --- regression guards: object literal + user class (already worked) ---
const o = { greet() { return "hi"; } };
print("obj=" + (o as any)["gre" + "et"]());

class C {
  hello(): string { return "yo"; }
  twice(x: number): number { return x * 2; }
}
const c = new C();
print("cls=" + (c as any)["hel" + "lo"]());
print("clsArg=" + (c as any)["twi" + "ce"](21));

// --- inside a loop: the same site sees several runtime names ---
const names = ["toUpperCase", "toLowerCase", "trim"];
let looped = "";
for (let i = 0; i < names.length; i++) {
  looped += (" Xy " as any)[names[i]]() + "|";
}
print("loop=" + looped);

describe("computed member access on primitive receivers", () => {
  test("string method by runtime name, no args", () =>
    expect(out.indexOf("upper=ABC\n") >= 0).toBe(true));

  test("string method by runtime name, with args", () =>
    expect(out.indexOf("slice=b\n") >= 0).toBe(true));

  test("computed property read on a primitive", () =>
    expect(out.indexOf("length=3\n") >= 0).toBe(true));

  test("numeric index is unaffected by the name path", () =>
    expect(out.indexOf("idx0=a\n") >= 0 && out.indexOf("idxStr1=b\n") >= 0).toBe(true));

  test("number receiver by runtime name", () =>
    expect(out.indexOf("radix=ff\n") >= 0).toBe(true));

  test("boolean receiver by runtime name", () =>
    expect(out.indexOf("bool=true\n") >= 0).toBe(true));

  test("absent name throws TypeError, absent property reads undefined", () =>
    expect(out.indexOf("missing=TypeError\n") >= 0
      && out.indexOf("missingRead=undefined\n") >= 0).toBe(true));

  test("object + user-class receivers still dispatch (regression guard)", () =>
    expect(out.indexOf("obj=hi\n") >= 0
      && out.indexOf("cls=yo\n") >= 0
      && out.indexOf("clsArg=42\n") >= 0).toBe(true));

  test("full expected stdout", () =>
    expect(out).toBe(
      "upper=ABC\n" +
      "trim=[pad]\n" +
      "slice=b\n" +
      "indexOf=2\n" +
      "length=3\n" +
      "methodIsFn=function\n" +
      "idx0=a\n" +
      "idxStr1=b\n" +
      "idxOOR=undefined\n" +
      "radix=ff\n" +
      "fixed=1.50\n" +
      "bool=true\n" +
      "missing=TypeError\n" +
      "missingRead=undefined\n" +
      "obj=hi\n" +
      "cls=yo\n" +
      "clsArg=42\n" +
      "loop=" + " XY |" + " xy |" + "Xy|" + "\n"
    ));
});
