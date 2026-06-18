// Faithful TypeScript `Number` — the REAL primordial Number for the new engine,
// written in `.ts` instead of hardcoded in codegen. Covers BOTH JS call forms
// from a SINGLE source (the array.ts pattern):
//
//   Number(value)      → a PRIMITIVE number   (the `NumberFactory` function)
//   new Number(value)  → a wrapper OBJECT      (the `class Number`, typeof "object")
//
// Number is a PRIMORDIAL (the engine may NAME it) and `number` is a PRIMITIVE
// (native literal syntax `123`), so the VALUE of the primitive stays an unboxed
// double/int. The engine routes the two JS forms here SYNTACTICALLY: a bare call
// `Number(x)` lowers to `NumberFactory`; `new Number(x)` constructs the class.
//
// ## Dual `this`: autoboxed primitive vs wrapper object
// A method can be reached on EITHER receiver and the engine is shape-based (NOT
// JS prototypes):
//   - `(5).toFixed(2)` — the PRIMITIVE is BOXED as `this` (autobox path,
//     `method::try_primitive_class_method`). `this` reads AS the number.
//   - `new Number(5).toFixed(2)` — `this` is the WRAPPER object; the primitive
//     lives in the `__prim` slot.
// The free helper `__num_val(self)` unifies both: `typeof self === "object"`
// (the wrapper) reads `self.__prim`; otherwise `self` IS the primitive.
//
// ## The irreducible formatting/parsing stays in Rust (one source of truth)
// JS number FORMATTING (float→string, radix, toFixed/toPrecision/toExponential)
// and string→number PARSING are delicate and implemented ONCE in Rust
// (`rts-primitives/src/number.rs`, the `__RTS_FN_GL_NUMBER_*` externs). These
// bodies do NOT reimplement them: they call the PRIVATE `engine.num_*` helpers,
// which wrap those exact Rust impls.

// Unwrap the primitive number from either an autoboxed primitive `self` (the
// number itself) or a wrapper-object `self` (its `__prim` slot).
function __num_val(self: any): number {
  if (typeof self === "object") {
    return self.__prim;
  }
  return self;
}

// `Number(value)` — coerce `value` to a PRIMITIVE number (no `new`). JS ToNumber:
// undefined → NaN, null → 0, boolean → 1/0, string → parsed (or NaN), number →
// itself, object → its valueOf/primitive (NaN when non-numeric).
function NumberFactory(value?: any): number {
  if (value === undefined) return 0 / 0; // NaN
  if (value === null) return 0;
  if (typeof value === "number") return value;
  if (typeof value === "boolean") return value ? 1 : 0;
  if (typeof value === "string") return engine.num_from_str(value);
  // object: unwrap a Number wrapper via its `__prim`; any other object is NaN.
  return __num_val(value);
}

class Number {
  // The wrapped primitive (only used by the `new Number(x)` object form).
  __prim: number;
  constructor(value?: any) {
    this.__prim = NumberFactory(value);
  }
  // The number itself (a number's primitive value is itself).
  valueOf(): number {
    return __num_val(this);
  }
  // Base-10 string by default; `radix` (2..36) selects another base. Delegates
  // to the Rust radix formatter (one source of truth).
  toString(radix: number = 10): string {
    return engine.num_to_string_radix(__num_val(this), radix);
  }
  // Locale string — no locale data in the runtime, so it defers to base-10
  // toString (matches the runtime's existing behavior).
  toLocaleString(): string {
    return engine.num_to_string_radix(__num_val(this), 10);
  }
  // Fixed-point notation with `digits` fraction digits (default 0).
  toFixed(digits: number = 0): string {
    return engine.num_to_fixed(__num_val(this), digits);
  }
  // `precision` significant digits (default: the auto sentinel -1, which the
  // Rust formatter renders as plain toString).
  toPrecision(precision: number = -1): string {
    return engine.num_to_precision(__num_val(this), precision);
  }
  // Exponential notation with `digits` mantissa fraction digits (default: the
  // auto sentinel -1, which picks the shortest faithful mantissa).
  toExponential(digits: number = -1): string {
    return engine.num_to_exponential(__num_val(this), digits);
  }
}
