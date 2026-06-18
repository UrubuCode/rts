// Faithful TypeScript `Boolean` — the REAL primordial Boolean for the new engine,
// written in `.ts` instead of hardcoded in codegen. Covers BOTH JS call forms
// from a SINGLE source (the array.ts pattern; mirrors number.ts):
//
//   Boolean(value)      → a PRIMITIVE boolean  (the `BooleanFactory` function)
//   new Boolean(value)  → a wrapper OBJECT      (the `class Boolean`, typeof "object")
//
// Boolean is a PRIMORDIAL (the engine may NAME it) and `boolean` is a PRIMITIVE
// (native literal syntax `true`/`false`). The engine routes the two JS forms here
// SYNTACTICALLY: a bare call `Boolean(x)` lowers to `BooleanFactory`; `new
// Boolean(x)` constructs the class.
//
// ## Dual `this`: autoboxed primitive vs wrapper object
// A method can be reached on EITHER receiver and the engine is shape-based (NOT
// JS prototypes):
//   - `true.toString()` — the PRIMITIVE is BOXED as `this` (autobox path,
//     `method::try_primitive_class_method`). `this` reads AS the boolean.
//   - `new Boolean(false).valueOf()` — `this` is the WRAPPER object; the
//     primitive lives in the `__prim` slot.
// The free helper `__bool_val(self)` unifies both: `typeof self === "object"`
// (the wrapper) reads `self.__prim`; otherwise `self` IS the primitive.
//
// ToBoolean is the engine's native truthiness (a value in a boolean position), so
// there is NO irreducible Rust helper here — `value ? true : false` is enough.

// Unwrap the primitive boolean from either an autoboxed primitive `self` (the
// boolean itself) or a wrapper-object `self` (its `__prim` slot).
function __bool_val(self: any): boolean {
  if (typeof self === "object") {
    return self.__prim;
  }
  return self;
}

// `Boolean(value)` — coerce `value` to a PRIMITIVE boolean (no `new`). JS
// ToBoolean: the engine's native truthiness test.
function BooleanFactory(value?: any): boolean {
  return value ? true : false;
}

class Boolean {
  // The wrapped primitive (only used by the `new Boolean(x)` object form).
  __prim: boolean;
  constructor(value?: any) {
    this.__prim = value ? true : false;
  }
  toString(): string {
    return __bool_val(this) ? "true" : "false";
  }
  valueOf(): boolean {
    return __bool_val(this);
  }
}
