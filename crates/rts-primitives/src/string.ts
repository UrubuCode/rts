// Faithful TypeScript `String` — the REAL primordial String for the new engine,
// written in `.ts` instead of hardcoded in codegen. Covers BOTH JS call forms
// from a SINGLE source (the array.ts pattern; mirrors number.ts / boolean.ts):
//
//   String(value)      → a PRIMITIVE string   (the `StringFactory` function)
//   new String(value)  → a wrapper OBJECT      (the `class String`, typeof "object")
//
// String is a PRIMORDIAL (the engine may NAME it) and `string` is a PRIMITIVE
// (native literal syntax `""`), so the VALUE of a primitive stays a `TAG_STR`
// PolyValue. The engine routes the two JS forms here SYNTACTICALLY: a bare call
// `String(x)` lowers to `StringFactory`; `new String(x)` constructs the class.
//
// ## Dual `this`: autoboxed primitive vs wrapper object
// A method can be reached on EITHER receiver and the engine is shape-based (NOT
// JS prototypes):
//   - `"abc".toUpperCase()` — the PRIMITIVE is BOXED as `this` (autobox path,
//     `method::try_primitive_class_method`). `this` reads AS the string.
//   - `new String("abc").toUpperCase()` — `this` is the WRAPPER object; the
//     primitive lives in the `__prim` slot.
// The free helper `__str_val(self)` unifies both: `typeof self === "object"`
// (the wrapper) reads `self.__prim`; otherwise `self` IS the primitive string.
//
// ## The irreducible Unicode logic + ToString stay in Rust (one source of truth)
// JS string semantics (Unicode case folding, trim, UTF-16 indexing, slice/pad/
// replace) is implemented ONCE in Rust (`rts-primitives/src/string/`, the
// `__RTS_FN_GL_STRING_*` externs) and reached via the PRIVATE `engine.str_*(this,
// ...args)` helpers. ToString of an arbitrary value (the factory / ctor coercion)
// uses the engine's own `engine.str_from_any` (the `__rtsadp_to_string`
// trampoline). NOTE: `String(obj)` of a STATICALLY-KNOWN-CLASS object still calls
// its custom `toString`/`valueOf` in the front-end (#304) BEFORE reaching the
// factory — so that ToPrimitive path is preserved.
//
// NOTE: `.length` is NOT here — on a proven string the engine reads it directly;
// `split` (returns an ARRAY) and the regex-first methods stay on the engine's
// `dispatch.rs`/`try_string_*` paths (array/regex marshaling is not a clean single
// string→string engine helper).

// NOTE on the repeated `2147483647` default below: it is the sentinel "to the end
// of the string" (a large i32 the Rust impl clamps to length). A shared top-level
// `const __STR_END` does NOT work here — the new engine does not yet resolve a
// prelude module-level const inside a class METHOD body (only top-level functions
// like `__str_val` reach method scope), so it would bail "unbound identifier". The
// literal is therefore the pragmatic single form until that scope gap is closed.

// Unwrap the primitive string from either an autoboxed primitive `self` (the
// string itself) or a wrapper-object `self` (its `__prim` slot).
function __str_val(self: any): string {
  if (typeof self === "object") {
    return self.__prim;
  }
  return self;
}

// `String(value)` — coerce `value` to a PRIMITIVE string (no `new`). JS ToString,
// delegating to the engine's own ToString (one source of truth). `String(undefined)`
// is "undefined" (NOT ""), so there is NO undefined short-circuit here — the
// engine's ToString renders it. (The zero-arg `String()` → "" stays on the engine
// fallback in `globals.rs`, which only routes the 1-arg form here.)
function StringFactory(value?: any): string {
  return engine.str_from_any(value);
}

class String {
  // The wrapped primitive (only used by the `new String(x)` object form).
  __prim: string;
  constructor(value?: any) {
    // `new String()` (no arg) → ""; `new String(x)` → ToString(x). The engine
    // can't distinguish a missing arg from an explicit `undefined`, so a literal
    // `new String(undefined)` also yields "" (a rare corner; documented).
    this.__prim = value === undefined ? "" : engine.str_from_any(value);
  }
  toString(): string {
    return __str_val(this);
  }
  valueOf(): string {
    return __str_val(this);
  }
  // `new String("ab").length` === 2. On a PRIMITIVE string `.length` is an
  // engine-direct read; on the WRAPPER object it routes to this getter, which
  // reads `.length` off the unwrapped primitive (the same engine-direct read).
  get length(): number {
    return __str_val(this).length;
  }
  toUpperCase(): string {
    return engine.str_to_upper(__str_val(this));
  }
  toLowerCase(): string {
    return engine.str_to_lower(__str_val(this));
  }
  // No locale data in the runtime — locale variants defer to the plain case
  // fold (matches the runtime's existing behavior).
  toLocaleUpperCase(): string {
    return engine.str_to_upper(__str_val(this));
  }
  toLocaleLowerCase(): string {
    return engine.str_to_lower(__str_val(this));
  }
  trim(): string {
    return engine.str_trim(__str_val(this));
  }
  trimStart(): string {
    return engine.str_trim_start(__str_val(this));
  }
  trimEnd(): string {
    return engine.str_trim_end(__str_val(this));
  }
  // trimLeft/trimRight are legacy aliases of trimStart/trimEnd.
  trimLeft(): string {
    return engine.str_trim_start(__str_val(this));
  }
  trimRight(): string {
    return engine.str_trim_end(__str_val(this));
  }
  charAt(i: number): string {
    return engine.str_char_at(__str_val(this), i);
  }
  charCodeAt(i: number): number {
    return engine.str_char_code_at(__str_val(this), i);
  }
  at(i: number): string {
    return engine.str_at(__str_val(this), i);
  }
  repeat(n: number): string {
    return engine.str_repeat(__str_val(this), n);
  }
  // slice/substring: the optional `end` defaults to a large sentinel the Rust
  // impl clamps to the string length ("to end").
  slice(start: number, end: number = 2147483647): string {
    return engine.str_slice(__str_val(this), start, end);
  }
  substring(start: number, end: number = 2147483647): string {
    return engine.str_substring(__str_val(this), start, end);
  }
  // indexOf takes an optional `position` (search start). Compose via slice: search
  // the tail from `position`, then offset the found index back to absolute (or keep
  // -1). `s.indexOf(x, p)` over the whole string when `p <= 0`.
  indexOf(needle: string, position: number = 0): number {
    if (position > 0) {
      const found = engine.str_index_of(__str_val(this.slice(position)), needle);
      return found < 0 ? -1 : found + position;
    }
    return engine.str_index_of(__str_val(this), needle);
  }
  // lastIndexOf takes an optional `fromIndex` — the LAST match that STARTS at index
  // <= fromIndex. Compose via slice: a match starting at <= fromIndex lies wholly
  // within `this.slice(0, fromIndex + needle.length)`, so lastIndexOf over that
  // prefix gives the same (absolute) index.
  lastIndexOf(needle: string, fromIndex: number = 2147483647): number {
    if (fromIndex < 2147483647) {
      const prefix = this.slice(0, fromIndex + needle.length);
      return engine.str_last_index_of(__str_val(prefix), needle);
    }
    return engine.str_last_index_of(__str_val(this), needle);
  }
  // includes/startsWith take an optional `position` (search start); endsWith an
  // optional `endPosition`. The irreducible search stays in the `engine.str_*`
  // bridge over the WHOLE string; the position is applied by composing with
  // `slice` (`s.includes(x, p) === s.slice(p).includes(x)`) — no new bridge.
  includes(needle: string, position: number = 0): boolean {
    if (position > 0) {
      return engine.str_includes(__str_val(this.slice(position)), needle);
    }
    return engine.str_includes(__str_val(this), needle);
  }
  startsWith(prefix: string, position: number = 0): boolean {
    if (position > 0) {
      return engine.str_starts_with(__str_val(this.slice(position)), prefix);
    }
    return engine.str_starts_with(__str_val(this), prefix);
  }
  endsWith(suffix: string, endPosition: number = 2147483647): boolean {
    if (endPosition < 2147483647) {
      return engine.str_ends_with(__str_val(this.slice(0, endPosition)), suffix);
    }
    return engine.str_ends_with(__str_val(this), suffix);
  }
  // padStart/padEnd: the pad string defaults to a single space (JS spec).
  padStart(targetLen: number, pad: string = " "): string {
    return engine.str_pad_start(__str_val(this), targetLen, pad);
  }
  padEnd(targetLen: number, pad: string = " "): string {
    return engine.str_pad_end(__str_val(this), targetLen, pad);
  }
  // concat folds up to four trailing args; empty-string defaults make the fold
  // an identity for the omitted slots (`"a".concat("b")` === "ab").
  concat(a: string = "", b: string = "", c: string = "", d: string = ""): string {
    let r = engine.str_concat(__str_val(this), a);
    r = engine.str_concat(r, b);
    r = engine.str_concat(r, c);
    r = engine.str_concat(r, d);
    return r;
  }
  // replace/replaceAll with a STRING search (a regex first arg is handled by the
  // engine's `try_string_regex_method` BEFORE this class is consulted).
  replace(from: string, to: string): string {
    return engine.str_replace(__str_val(this), from, to);
  }
  replaceAll(from: string, to: string): string {
    return engine.str_replace_all(__str_val(this), from, to);
  }
}
