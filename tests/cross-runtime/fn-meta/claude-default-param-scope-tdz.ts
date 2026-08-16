// Cross-runtime: when a function has default parameters, the parameter list is
// its OWN scope. A default may read parameters to its left, not to its right,
// and never the body's `var`s — the body gets a copy at entry.

// A default reads an earlier parameter.
function earlier(a: number, b: number = a * 2, c: number = a + b): string {
  return a + "/" + b + "/" + c;
}
console.log("earlier_all=" + earlier(1));
console.log("earlier_two=" + earlier(1, 10));
console.log("earlier_three=" + earlier(1, 10, 100));

// A default reading a LATER parameter hits the dead zone.
function later(a: number = (b as any), b: number = 2): string {
  return a + "/" + b;
}
try {
  console.log("later=" + later());
} catch (e) {
  console.log("later_threw=" + (e as any).constructor.name);
}
// Supplying the value skips the default, so nothing throws.
console.log("later_supplied=" + later(9));

// A parameter cannot see its own initialiser.
function selfRef(a: number = (a as any)): string { return String(a); }
try {
  console.log("self_ref=" + selfRef());
} catch (e) {
  console.log("self_ref_threw=" + (e as any).constructor.name);
}

// The body's `var` of the same name starts as a COPY of the parameter.
function bodyVar(a: number = 5): string {
  var a2 = a;
  var a3;
  a3 = a;
  a = 99;
  return a + "/" + a2 + "/" + a3;
}
console.log("body_var_copy=" + bodyVar());

// A default cannot see a `var` declared in the body.
function seesBodyVar(a: any = typeof hidden): string {
  var hidden = "body-value";
  return String(a) + "/" + hidden;
}
console.log("default_vs_body_var=" + seesBodyVar());

// With defaults present, `arguments` is UNMAPPED: writing the parameter does
// not change arguments[0] and vice versa.
function unmapped(a: number = 0): string {
  const before = String(arguments[0]);
  a = 111;
  const after = String(arguments[0]);
  (arguments as any)[0] = 222;
  return before + "/" + after + "/" + a;
}
console.log("unmapped_with_default=" + unmapped(7));

// A rest parameter also unmaps `arguments`.
function restUnmapped(...rest: number[]): string {
  const before = String(arguments[0]);
  rest[0] = 111;
  return before + "/" + String(arguments[0]) + "/" + rest[0];
}
console.log("unmapped_with_rest=" + restUnmapped(7));

// A destructuring parameter unmaps it too.
function destructuredUnmapped({ v }: any = { v: 0 }): string {
  const before = JSON.stringify(arguments[0]);
  return before + "/" + v;
}
console.log("unmapped_with_pattern=" + destructuredUnmapped({ v: 3 }));

// A closure created in a default captures the PARAMETER scope, not the body.
function capturesParamScope(a: number = 1, get: () => number = () => a): string {
  a = 50;
  return String(get());
}
console.log("closure_in_default=" + capturesParamScope());

// The same closure, when the body redeclares with `let`, still sees the param.
function paramScopeVsLet(a: number = 1, get: () => number = () => a): string {
  let shadow = a + 100;
  return get() + "/" + shadow;
}
console.log("param_scope_vs_let=" + paramScopeVsLet(2));

// Defaults run left to right, once, only for missing arguments.
const calls: string[] = [];
function trace(tag: string, v: any): any { calls.push(tag); return v; }
function ordered(
  a: any = trace("a", 1),
  b: any = trace("b", 2),
  c: any = trace("c", 3),
): string {
  return a + "" + b + "" + c;
}
calls.length = 0;
console.log("all_defaults=" + ordered() + " calls=" + calls.join(","));
calls.length = 0;
console.log("middle_given=" + ordered(undefined, 9) + " calls=" + calls.join(","));
calls.length = 0;
console.log("explicit_undefined=" + ordered(undefined, undefined, 9) + " calls=" + calls.join(","));
calls.length = 0;
console.log("null_skips_default=" + ordered(null, null, null) + " calls=" + calls.join(","));

// A default expression may call a function declared later at module level.
function usesHoisted(a: number = helper()): number { return a; }
function helper(): number { return 77; }
console.log("hoisted_helper=" + usesHoisted());

// Destructuring parameters get defaults at both levels.
function destructured({ x = 1, y = x + 1 }: any = {}, [p = y] : any = []): string {
  return x + "/" + y + "/" + p;
}
console.log("destructured_all=" + destructured());
console.log("destructured_partial=" + destructured({ x: 5 }));
console.log("destructured_both=" + destructured({ x: 5, y: 6 }, [7]));

// `length` stops at the first default and ignores rest.
console.log("length_defaults=" + ordered.length);
console.log("length_earlier=" + earlier.length);
function withRest(a: number, b: number = 1, ...rest: number[]): number { return a + b + rest.length; }
console.log("length_rest=" + withRest.length);

// A rest parameter is a fresh Array each call, never `arguments`.
function restIdentity(...rest: number[]): string {
  return Array.isArray(rest) + "/" + (rest === (arguments as any)) + "/" + rest.length;
}
console.log("rest_identity=" + restIdentity(1, 2, 3));
