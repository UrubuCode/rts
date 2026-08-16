// Cross-runtime: the `catch` parameter is a binding of its own block — it
// shadows an outer name, assignment to it never leaks out, it can be a
// destructuring pattern with defaults, and each entry to the catch makes a
// fresh cell.

// 1) The catch binding shadows an outer `let` and restores it on exit.
let err = "outer-value";
try {
  throw "thrown-value";
} catch (err) {
  console.log("inside=" + err);
  err = "reassigned-inside";
  console.log("after_assign_inside=" + err);
}
console.log("outside=" + err);

// 2) A `let` declared in the catch BODY shadows the parameter itself.
try {
  throw "param";
} catch (e) {
  {
    let e = "block-local";
    console.log("body_shadow=" + e);
  }
  console.log("param_intact=" + e);
}

// 3) Destructuring catch parameter, with a default for a missing member.
try {
  throw { code: 42 };
} catch ({ code, kind = "unknown" }: any) {
  console.log("destructured=" + code + "/" + kind);
}

// 4) Array pattern with a hole and a rest element.
try {
  throw [1, 2, 3, 4];
} catch ([, second, ...others]: any) {
  console.log("array_pattern=" + second + "|" + others.join("-"));
}

// 5) Nested pattern with a computed key.
const KEY = "detail";
try {
  throw { [KEY]: { line: 7, col: 3 } };
} catch ({ [KEY]: { line, col } }: any) {
  console.log("nested_pattern=" + line + ":" + col);
}

// 6) The default in a catch pattern is evaluated only when the member is
//    absent, and it can call a function.
const defaultsRun: string[] = [];
function fallback(name: string): string {
  defaultsRun.push(name);
  return "fb-" + name;
}
try {
  throw { a: "present" };
} catch ({ a = fallback("a"), b = fallback("b") }: any) {
  console.log("defaults=" + a + "|" + b);
}
console.log("defaults_run=" + defaultsRun.join(","));

// 7) Each entry to the catch creates a NEW binding, so closures made in
//    different entries do not share a cell.
const captured: Array<() => string> = [];
for (let i = 0; i < 3; i++) {
  try {
    throw "e" + i;
  } catch (caught) {
    captured.push(() => caught);
  }
}
console.log("per_entry=" + captured.map((f) => f()).join(","));

// 8) Mutating the binding after the closure was made is visible to it — one
//    cell per entry, not one snapshot per read.
function mutatedBinding(): string {
  try {
    throw "start";
  } catch (c) {
    const read = () => c;
    c = "mutated";
    return read();
  }
}
console.log("mutated_binding=" + mutatedBinding());

// 9) A nested try inside the catch shadows the outer catch's binding.
function nestedCatch(): string {
  try {
    throw "outer-error";
  } catch (e) {
    let inner = "?";
    try {
      throw "inner-error";
    } catch (e) {
      inner = e as string;
    }
    return e + "|" + inner;
  }
}
console.log("nested_catch=" + nestedCatch());

// 10) A `var` in the catch body is function-scoped and outlives the block; the
//     catch parameter does not.
function varEscapes(): string {
  try {
    throw "boom";
  } catch (e) {
    var leaked = "var-from-catch:" + e;
  }
  return leaked + "|param_visible=" + (typeof (globalThis as any).e === "undefined");
}
console.log("var_escapes=" + varEscapes());

// 11) The pattern's own bindings are block-scoped to the catch as well.
function patternScope(): string {
  const outside: string[] = [];
  try {
    throw { name: "pattern" };
  } catch ({ name }: any) {
    outside.push(name);
  }
  return outside.join(",") + "|name_here=" + typeof (globalThis as any).name_probe;
}
console.log("pattern_scope=" + patternScope());

// 12) Throwing a primitive: the binding holds it verbatim, no boxing.
const kinds: string[] = [];
const thrown: any[] = [0, "", false, null, undefined, NaN];
for (const v of thrown) {
  try {
    throw v;
  } catch (e) {
    kinds.push(typeof e + ":" + String(e) + ":" + (e === v ? "same" : "changed"));
  }
}
console.log("primitives=" + kinds.join(" "));

// 13) Rethrowing from the catch preserves identity.
const marker = { id: "the-one" };
function rethrow(): string {
  try {
    try {
      throw marker;
    } catch (e) {
      throw e;
    }
  } catch (e) {
    return String(e === marker) + ":" + (e as any).id;
  }
}
console.log("rethrow_identity=" + rethrow());

// 14) The catch parameter can be reassigned and the change is seen by the rest
//     of the block, including a nested function called later.
function reassignedThenCalled(): string {
  try {
    throw "first";
  } catch (e) {
    const show = function (): string { return String(e); };
    const before = show();
    e = "second";
    return before + "->" + show();
  }
}
console.log("reassigned_then_called=" + reassignedThenCalled());

// 15) A parameter pattern that destructures a STRING: strings are array-like
//     only through iteration, so an array pattern takes characters.
try {
  throw "xyz";
} catch ([first, ...tail]: any) {
  console.log("string_pattern=" + first + "|" + tail.join(""));
}

// 16) An object pattern over a thrown Error reads its own field, never its
//     message, and the constructor is still visible.
try {
  throw Object.assign(new RangeError("ignored"), { where: "line-9" });
} catch (e) {
  const { where } = e as any;
  console.log("error_field=" + where);
  console.log("error_ctor=" + (e as any).constructor.name);
}

// 17) The catch of an inner try that itself has a finally: the binding is in
//     scope for the catch only, not the finally.
function catchVsFinallyScope(): string {
  const seen: string[] = [];
  try {
    throw "scoped";
  } catch (e) {
    seen.push("catch-sees:" + e);
  } finally {
    seen.push("finally-sees:" + typeof (globalThis as any).e_probe);
  }
  return seen.join(",");
}
console.log("catch_vs_finally=" + catchVsFinallyScope());

// 18) Shadowing a function parameter with the catch binding.
function shadowsParam(e: string): string {
  const before = e;
  try {
    throw "inner";
  } catch (e) {
    return before + "|" + e;
  }
}
console.log("shadows_param=" + shadowsParam("param-value"));

// 19) The pattern's defaults may reference bindings declared earlier in the
//     same pattern.
try {
  throw { base: 10 };
} catch ({ base, scaled = base * 3 }: any) {
  console.log("pattern_default_uses_prior=" + base + "/" + scaled);
}

// 20) A catch parameter shadowed inside a nested arrow's own parameter list.
function arrowShadow(): string {
  try {
    throw "outerc";
  } catch (c) {
    const f = (c: string): string => "arrow:" + c;
    return f("innerc") + "|catch:" + c;
  }
}
console.log("arrow_shadow=" + arrowShadow());
