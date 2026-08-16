// Cross-runtime: the name of a named function EXPRESSION lives in a scope of
// its own — visible inside the body, invisible outside, and immutable, so an
// assignment to it throws in strict code (this module is strict).

const fact = function factorial(n: number): number {
  return n < 2 ? 1 : n * factorial(n - 1);
};
console.log("recursion=" + fact(6));
console.log("outer_typeof=" + typeof (globalThis as any).factorial);
console.log("self_name=" + fact.name);

// The binding is immutable: a write never takes effect.
const unchanged = function keeper(): string {
  try {
    (keeper as any) = null;
  } catch {
    /* a refusal is one way to say no */
  }
  return typeof keeper;
};
console.log("still_function=" + unchanged());

// Even after the failed write it still recurses through its own name.
const stillRecurses = function walk(n: number): number {
  try {
    (walk as any) = null;
  } catch {
    /* ignored */
  }
  return n <= 0 ? 0 : 1 + walk(n - 1);
};
console.log("still_recurses=" + stillRecurses(4));

// The binding is not configurable away either.
const notDeletable = function target(): string {
  return typeof target;
};
console.log("still_bound=" + notDeletable());

// The self-binding survives the outer binding being reassigned.
let outer = function inner(n: number): number {
  return n <= 0 ? 0 : n + inner(n - 1);
};
const captured = outer;
outer = function (): number { return -1; };
console.log("survives_reassign=" + captured(4));
console.log("outer_now=" + outer());

// A PARAMETER of the same name shadows the self-binding.
const shadowedByParam = function me(me: any): string {
  return typeof me;
};
console.log("param_shadows=" + shadowedByParam(5));
console.log("param_shadows_fn=" + shadowedByParam(function () {}));

// A `var` of the same name in the body shadows it too, and IS writable.
const shadowedByVar = function me(): string {
  var me: any = "local";
  me = "reassigned";
  return me;
};
console.log("var_shadows=" + shadowedByVar());

// A `let` of the same name in the body shadows it as well.
const shadowedByLet = function me(): string {
  let me = "let-value";
  return me;
};
console.log("let_shadows=" + shadowedByLet());

// The self-binding is visible in a nested function and in a default parameter.
const nested = function outerName(n: number): string {
  const helper = (): string => typeof outerName;
  return helper() + ":" + n;
};
console.log("nested_sees=" + nested(1));

const inDefault = function d(x: any = typeof d): string { return String(x); };
console.log("default_sees=" + inDefault());

// The declared name wins over the binding name.
const alias = function realName(): void {};
console.log("name_is_declared=" + alias.name);

// A named function expression as an object member.
const holder = {
  method: function memberName(n: number): number {
    return n === 0 ? 0 : 1 + memberName(n - 1);
  },
};
console.log("member_recursion=" + holder.method(5));
console.log("member_name=" + holder.method.name);
console.log("member_leaked=" + typeof (globalThis as any).memberName);

// A named function expression passed straight into a call.
console.log("inline=" + (function ticker(n: number): string {
  return n === 0 ? "0" : n + ">" + ticker(n - 1);
})(4));

// An IIFE that recurses by its own name, twice, to show the binding is fresh
// per CALL of the outer expression but the same object within one call.
function makeCounter(): () => number {
  let calls = 0;
  return function tick(): number {
    calls += 1;
    return calls < 3 ? tick() : calls;
  };
}
const c1 = makeCounter();
const c2 = makeCounter();
console.log("counter1=" + c1());
console.log("counter2=" + c2());
console.log("counters_distinct=" + (c1 !== c2));

// A named CLASS expression behaves the same way: the name is bound inside.
const Klass = class Inner {
  static describe(): string { return "inside:" + Inner.name; }
};
console.log("class_expr_inside=" + Klass.describe());
console.log("class_expr_name=" + Klass.name);
console.log("class_expr_leaked=" + typeof (globalThis as any).Inner);

// The class-expression name binding is const, so assigning to it throws.
const Stubborn = class Self {
  static poke(): string {
    try {
      (Self as any) = 1;
      return "assigned";
    } catch (e) {
      return "threw:" + (e as any).constructor.name;
    }
  }
};
console.log("class_assignment=" + Stubborn.poke());

// A declaration, by contrast, IS an ordinary writable binding.
function declared(): string { return "original"; }
const keep = declared;
// eslint-disable-next-line no-func-assign
(declared as any) = function (): string { return "replaced"; };
console.log("declaration_writable=" + declared());
console.log("original_kept=" + keep());
