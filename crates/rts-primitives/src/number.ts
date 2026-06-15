// Faithful TypeScript `Number.prototype` methods — the REAL primordial Number
// method library for the new engine, written in `.ts` instead of hardcoded in
// codegen.
//
// Number is a PRIMORDIAL (the engine may NAME it) and `number` is a PRIMITIVE
// (native literal syntax `123`), so the VALUE stays an unboxed double/int — only
// its METHOD LIBRARY moves here. The engine compiles this declarations-only
// prelude ahead of the user program; its ambient `class Number` supplies the
// prototype methods.
//
// ## The mechanism (same as boolean.ts): primitive → prelude-`.ts`-class dispatch
// A method called on a PRIMITIVE number receiver (`(5).toFixed(2)`,
// `(255).toString(16)`) is routed by the engine into THIS class's method, with
// the primitive number BOXED as the method's `this`. This is NOT JS prototypes:
// the engine is shape-based; it resolves the (method, arity) on the ambient
// `class Number` at compile time (the same `try_class_method` path a user-class
// instance uses) and passes the boxed primitive as the implicit `this`.
//
// Therefore the method bodies read `this` AS THE PRIMITIVE number. There are NO
// fields and NO constructor: this class only carries prototype methods.
//
// ## The irreducible formatting stays in Rust (one source of truth)
// JS number FORMATTING (float→string, radix, toFixed/toPrecision/toExponential)
// is delicate and already implemented ONCE in Rust (`rts-primitives/src/number.rs`,
// the `__RTS_FN_GL_NUMBER_*` externs). These bodies do NOT reimplement it: they
// call the PRIVATE `engine.num_*(this, arg)` helpers, which wrap those exact Rust
// formatters. `valueOf()` is the one pure body (a number is its own primitive).

class Number {
  // The number itself (a number's primitive value is itself).
  valueOf(): number {
    return this;
  }
  // Base-10 string by default; `radix` (2..36) selects another base. Delegates
  // to the Rust radix formatter (one source of truth).
  toString(radix: number = 10): string {
    return engine.num_to_string_radix(this, radix);
  }
  // Locale string — no locale data in the runtime, so it defers to base-10
  // toString (matches the runtime's existing behavior).
  toLocaleString(): string {
    return engine.num_to_string_radix(this, 10);
  }
  // Fixed-point notation with `digits` fraction digits (default 0).
  toFixed(digits: number = 0): string {
    return engine.num_to_fixed(this, digits);
  }
  // `precision` significant digits (default: the auto sentinel -1, which the
  // Rust formatter renders as plain toString).
  toPrecision(precision: number = -1): string {
    return engine.num_to_precision(this, precision);
  }
  // Exponential notation with `digits` mantissa fraction digits (default: the
  // auto sentinel -1, which picks the shortest faithful mantissa).
  toExponential(digits: number = -1): string {
    return engine.num_to_exponential(this, digits);
  }
}
