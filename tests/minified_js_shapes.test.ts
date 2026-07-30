// Shapes a MINIFIER / bundler emits that plain hand-written source rarely does.
// Each block below was a real compile-time bail before the fixes:
//   1. JS arity: a call with FEWER args than declared params (a required param is
//      only "required" in TS — in JS it is `undefined`).
//   2. An unbound identifier read: a RUNTIME `ReferenceError`, not a compile
//      error. UMD wrappers name `exports`/`module`/`define` in dead branches.
//   3. An arrow / function expression inside a SEQUENCE (comma operator) — how a
//      minifier folds several statements into one expression.
import { describe, test, expect } from "rts:test";

// --- 1. arity ---------------------------------------------------------------
function add2(a: any, b: any) {
  return b === undefined ? "no-b" : a + b;
}
const arityDecl = add2(1);
const arityExpr = (function (a: any, b: any) {
  return b === undefined ? "no-b" : a + b;
})(1);
const arityFull = add2(1, 2);

// --- 2. UMD feature sniff (typeof of names that do not exist here) -----------
let umdTarget: any = {};
!(function (root: any, factory: any) {
  if (typeof exports === "object") {
    factory(exports);
  } else {
    factory((root.L = {}));
  }
})(umdTarget, function (e: any) {
  e.v = "1.0";
  e.f = function (t: any) {
    return t * 2;
  };
});
const umdVersion = umdTarget.L.v;
const umdDoubled = umdTarget.L.f(21);

// A truly unbound READ is a catchable ReferenceError, not a compile failure.
let refErr = "none";
try {
  const dead: any = (globalThis as any)["x"] ? 0 : notDefinedAnywhere;
  refErr = "" + dead;
} catch (e: any) {
  refErr = e.name;
}

// --- 3. arrow inside a comma sequence ---------------------------------------
const seqHost: any = {};
((seqHost.a = 1), (seqHost.b = function (x: any) {
  return x + 1;
}), (seqHost.c = (x: any) => x * 3));
const seqB = seqHost.b(1);
const seqC = seqHost.c(3);

// --- 4. object-literal METHODS built inside an INLINE factory ---------------
// The module/UMD pattern. The literal's methods were recovered while the factory
// was still an inline arrow, so they had to survive the later lift — including a
// method that CLOSES OVER a local of the factory.
function applyFactory(f: any) {
  return f();
}
const modLib: any = applyFactory(function () {
  const cache: any = { n: 0 };
  return {
    bump(by: number) {
      cache.n = cache.n + by;
      return cache.n;
    },
    read() {
      return cache.n;
    },
  };
});
const modType = typeof modLib.bump;
const modBumped = modLib.bump(4) + modLib.bump(3);
const modRead = modLib.read();

// The paired DESYNC guard: a literal built inside an inline arrow used to shift
// the swc↔HIR pairing, so the NEXT literal inherited the arrow literal's class.
applyFactory(function () {
  return { hidden() { return 1; } };
});
const afterArrow: any = { own(): number { return 2; } };
const afterArrowOk = typeof afterArrow.own === "function" && afterArrow.own() === 2;

// --- 5. spread / wide arity through a DYNAMIC callee ------------------------
// The opcode-table dispatch (`_ops[code](...args)`) an obfuscator emits.
const opTable: any = {
  sum: function (a: number, b: number, c: number) { return a + b + c; },
};
const spreadArgs: any = [1, 2, 3];
const viaMember = opTable.sum(...spreadArgs);
const viaIndex = opTable["sum"](...spreadArgs);
const wideArity = (function (o: any) {
  return o["sum"](1, 2, 3);
})(opTable);

// --- 6. `var`-declared array keeps its array-ness ---------------------------
// `var` hoisting splits declaration from initializer, so the array arrives as an
// ASSIGNMENT. Minified code declares everything with `var`.
var varArr = ["delta", "alpha", "charlie"];
const varSliceSorted = varArr.slice().sort().join(",");
let lateArr: any;
lateArr = [3, 1, 2];
const lateSorted = lateArr.slice().sort().join(",");

// --- 7. `arguments` sees the args actually passed -------------------------
// `Array.prototype.slice.call(arguments, 1)` — the ES5 forwarding idiom every
// transpiler emits — reads exactly the args PAST the declared ones.
function forwardTail(first: any) {
  return Array.prototype.slice.call(arguments, 1).join(",");
}
const argTail = forwardTail(1, 2, 3);
function argCount(a: any, b: any) {
  return arguments.length;
}
const argOver = argCount(1, 2, 3, 4);
const argExact = argCount(1, 2);
// `length` ignores the synthesized rest (and a real one — the spec rule).
function declaredTwo(a: any, b: any) {
  return arguments.length;
}
function realRest(a: any, ...r: any[]) {
  return r.length;
}
const lenDeclared = (declaredTwo as any).length;
const lenRest = (realRest as any).length;

// --- 8. `this` in a plain function expression -------------------------------
// The ES5 constructor / prototype-method shape a bundle is built from.
const litRecv: any = { o: 5, n: function () { return this.o; } };
const litThis = litRecv.n();
const bump: any = { c: 0, inc: function () { this.c++; return this.c; } };
const bumpTwice = bump.inc() + bump.inc();
function Base(this: any) {
  this.type = "base";
}
(Base as any).prototype.greet = function () {
  return "hi " + this.type;
};
const baseInst: any = new (Base as any)();
const protoThis = baseInst.type + "/" + baseInst.greet();

// --- 9. array separator that is not statically a string ---------------------
const joinVia: any = function (s: string, sep: any) {
  return s.split("").join(sep);
};
const joinDyn = joinVia("abc", "-");
const joinNoSep = joinVia("abc");
let sepUndef: any;
const joinUndef = [1, 2].join(sepUndef);
const joinNumeric = [1, 2, 3].join(0);

// --- 10. `new <expression>` (no class NAME to resolve) ----------------------
const anonNew: any = new (function (this: any) {
  this.z = 8;
})();
const AnonA: any = function (this: any, v: any) {
  this.v = v;
};
const AnonB: any = function (this: any) {
  this.v = -1;
};
const pickedNew: any = new (1 ? AnonA : AnonB)(7);
const newReturnsObject: any = new (function (this: any) {
  this.a = 1;
  return { a: 99 };
} as any)();

describe("minified/obfuscated JS shapes", () => {
  test("omitted arg on a function DECLARATION is undefined", () =>
    expect(arityDecl).toBe("no-b"));
  test("omitted arg on a function EXPRESSION is undefined", () =>
    expect(arityExpr).toBe("no-b"));
  test("the full-arity call still works", () => expect(arityFull).toBe(3));

  test("UMD sniff takes the non-exports branch", () =>
    expect(umdVersion).toBe("1.0"));
  test("the UMD factory's nested fn-expr is callable", () =>
    expect(umdDoubled).toBe(42));
  test("reading an unbound name throws ReferenceError", () =>
    expect(refErr).toBe("ReferenceError"));

  test("fn-expr inside a comma sequence lifts", () => expect(seqB).toBe(2));
  test("arrow inside a comma sequence lifts", () => expect(seqC).toBe(9));

  test("a literal built in an inline factory keeps its methods", () =>
    expect(modType).toBe("function"));
  test("such a method closes over the factory's local", () =>
    expect(modBumped).toBe(11));
  test("and the closed-over state is shared between methods", () =>
    expect(modRead).toBe(7));
  test("a literal after an arrow-built one keeps its OWN class", () =>
    expect(afterArrowOk).toBe(true));

  test("spread through a member callee", () => expect(viaMember).toBe(6));
  test("spread through a computed-index callee", () => expect(viaIndex).toBe(6));
  test("more args than the fixed dynamic-call slots", () =>
    expect(wideArity).toBe(6));

  test("`var`-declared array keeps its array methods", () =>
    expect(varSliceSorted).toBe("alpha,charlie,delta"));
  test("declare-then-assign array keeps its array methods", () =>
    expect(lateSorted).toBe("1,2,3"));

  test("`arguments` carries the args past the declared params", () =>
    expect(argTail).toBe("2,3"));
  test("`arguments.length` counts an over-applied call", () =>
    expect(argOver).toBe(4));
  test("`arguments.length` on an exact call", () => expect(argExact).toBe(2));
  test("`length` reports the declared param count", () =>
    expect(lenDeclared).toBe(2));
  test("`length` ignores a rest param (spec)", () => expect(lenRest).toBe(1));

  test("`this` in an object-literal fn-expr method", () =>
    expect(litThis).toBe(5));
  test("`this` mutated through a fn-expr method", () =>
    expect(bumpTwice).toBe(3));
  test("ES5 constructor + prototype method bind `this`", () =>
    expect(protoThis).toBe("base/hi base"));

  test("join with an any-typed separator", () => expect(joinDyn).toBe("a-b-c"));
  test("join with the separator omitted", () => expect(joinNoSep).toBe("a,b,c"));
  test("join(undefined) uses the default separator (spec)", () =>
    expect(joinUndef).toBe("1,2"));
  test("join coerces a non-string separator", () =>
    expect(joinNumeric).toBe("10203"));

  test("`new` on an anonymous fn-expr binds `this`", () =>
    expect(anonNew.z).toBe(8));
  test("`new` on a ternary-selected constructor", () =>
    expect(pickedNew.v).toBe(7));
  test("a constructor returning an object wins (spec)", () =>
    expect(newReturnsObject.a).toBe(99));
});
