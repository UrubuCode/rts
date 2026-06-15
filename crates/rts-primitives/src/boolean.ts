// Faithful TypeScript `Boolean.prototype` methods — the REAL primordial Boolean
// method library for the new engine, written in `.ts` instead of hardcoded in
// codegen.
//
// Boolean is a PRIMORDIAL (the engine may NAME it), but its METHOD bodies live
// here, not in Rust. The engine compiles this declarations-only prelude ahead of
// the user program; its ambient `class Boolean` supplies the prototype methods.
//
// ## The mechanism this file proves: primitive → prelude-`.ts`-class dispatch
// A method called on a PRIMITIVE bool receiver (`true.toString()`,
// `false.valueOf()`) is routed by the engine into THIS class's method, with the
// primitive boolean BOXED as the method's `this`. This is NOT JS prototypes: the
// engine is shape-based; it resolves the (method, arity) on the ambient
// `class Boolean` at compile time (the same `try_class_method` path a user-class
// instance uses) and passes the boxed primitive as the implicit `this`.
//
// Therefore the method bodies read `this` AS THE PRIMITIVE boolean — `this` is
// the boxed bool word, used directly in a truthiness test / returned as-is. There
// are NO fields and NO constructor: this class only carries prototype methods.
// The wrapper object form `new Boolean(x)` (typeof === "object") is NOT this
// class — it stays the engine's wrapper trampoline (a later increment moves it
// here too).
//
// `Boolean.prototype.toString()` → "true" / "false" (JS spec).
// `Boolean.prototype.valueOf()` → the primitive boolean itself.

class Boolean {
  toString(): string {
    return this ? "true" : "false";
  }
  valueOf(): boolean {
    return this ? true : false;
  }
}
