// Faithful TypeScript `String.prototype` methods — the REAL primordial String
// method library for the new engine, written in `.ts` instead of hardcoded in
// codegen.
//
// String is a PRIMORDIAL (the engine may NAME it) and `string` is a PRIMITIVE
// (native literal syntax `""`), so the VALUE stays a `TAG_STR` PolyValue (a
// string-pool handle) — only its METHOD LIBRARY moves here. The engine compiles
// this declarations-only prelude ahead of the user program; its ambient
// `class String` supplies the prototype methods.
//
// ## The mechanism (same as boolean.ts / number.ts): primitive → prelude-`.ts`-
// ## class dispatch
// A method called on a PRIMITIVE string receiver (`"abc".toUpperCase()`,
// `"  x ".trim()`) is routed by the engine into THIS class's method, with the
// primitive string BOXED as the method's `this`. This is NOT JS prototypes: the
// engine is shape-based; it resolves the (method, arity) on the ambient
// `class String` at compile time (the same `try_class_method` path a user-class
// instance uses) and passes the boxed primitive as the implicit `this`.
//
// Therefore the method bodies read `this` AS THE PRIMITIVE string. There are NO
// fields and NO constructor: this class only carries prototype methods.
//
// ## The irreducible Unicode logic stays in Rust (one source of truth)
// JS string semantics (Unicode case folding, trim, UTF-16 code-unit indexing,
// slice/substring by char, pad, replace) is delicate and already implemented
// ONCE in Rust (`rts-primitives/src/string/`, the `__RTS_FN_GL_STRING_*`
// externs). These bodies do NOT reimplement it: they call the PRIVATE
// `engine.str_*(this, ...args)` helpers, which wrap those exact Rust impls.
//
// NOTE: `.length` is NOT here — on a proven string the engine reads it directly
// (`obj.rs` → `__rtsadp_dyn_length`), and the class system has no primitive
// `length` getter hook, so adding one here would be dead. `split` (returns an
// ARRAY) and the regex-first methods (`match`/`search`/regex `replace`/`split`)
// stay on the engine's `dispatch.rs`/`try_string_*` paths — array/regex
// marshaling through a single string→string engine helper is not clean.

class String {
  toUpperCase(): string {
    return engine.str_to_upper(this);
  }
  toLowerCase(): string {
    return engine.str_to_lower(this);
  }
  // No locale data in the runtime — locale variants defer to the plain case
  // fold (matches the runtime's existing behavior).
  toLocaleUpperCase(): string {
    return engine.str_to_upper(this);
  }
  toLocaleLowerCase(): string {
    return engine.str_to_lower(this);
  }
  trim(): string {
    return engine.str_trim(this);
  }
  trimStart(): string {
    return engine.str_trim_start(this);
  }
  trimEnd(): string {
    return engine.str_trim_end(this);
  }
  // trimLeft/trimRight are legacy aliases of trimStart/trimEnd.
  trimLeft(): string {
    return engine.str_trim_start(this);
  }
  trimRight(): string {
    return engine.str_trim_end(this);
  }
  charAt(i: number): string {
    return engine.str_char_at(this, i);
  }
  charCodeAt(i: number): number {
    return engine.str_char_code_at(this, i);
  }
  at(i: number): string {
    return engine.str_at(this, i);
  }
  repeat(n: number): string {
    return engine.str_repeat(this, n);
  }
  // slice/substring: the optional `end` defaults to a large sentinel the Rust
  // impl clamps to the string length ("to end").
  slice(start: number, end: number = 2147483647): string {
    return engine.str_slice(this, start, end);
  }
  substring(start: number, end: number = 2147483647): string {
    return engine.str_substring(this, start, end);
  }
  indexOf(needle: string): number {
    return engine.str_index_of(this, needle);
  }
  lastIndexOf(needle: string): number {
    return engine.str_last_index_of(this, needle);
  }
  includes(needle: string): boolean {
    return engine.str_includes(this, needle);
  }
  startsWith(prefix: string): boolean {
    return engine.str_starts_with(this, prefix);
  }
  endsWith(suffix: string): boolean {
    return engine.str_ends_with(this, suffix);
  }
  // padStart/padEnd: the pad string defaults to a single space (JS spec).
  padStart(targetLen: number, pad: string = " "): string {
    return engine.str_pad_start(this, targetLen, pad);
  }
  padEnd(targetLen: number, pad: string = " "): string {
    return engine.str_pad_end(this, targetLen, pad);
  }
  // concat folds up to four trailing args; empty-string defaults make the fold
  // an identity for the omitted slots (`"a".concat("b")` === "ab").
  concat(a: string = "", b: string = "", c: string = "", d: string = ""): string {
    let r = engine.str_concat(this, a);
    r = engine.str_concat(r, b);
    r = engine.str_concat(r, c);
    r = engine.str_concat(r, d);
    return r;
  }
  // replace/replaceAll with a STRING search (a regex first arg is handled by the
  // engine's `try_string_regex_method` BEFORE this class is consulted).
  replace(from: string, to: string): string {
    return engine.str_replace(this, from, to);
  }
  replaceAll(from: string, to: string): string {
    return engine.str_replace_all(this, from, to);
  }
}
