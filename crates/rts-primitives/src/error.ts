// Faithful TypeScript `Error` family — the REAL primordial Error classes for the
// new engine.
//
// Error is a PRIMORDIAL (the engine may NAME it for throw/catch), but its
// FIELD/METHOD IMPLEMENTATION lives here in `.ts`, not hardcoded in codegen. The
// engine compiles this ahead of the user program as a declarations-only prelude
// `include`; its top-level `class Error` (+ subclasses) become ambient and the
// user's `new Error("x")` constructs THIS class (a shape-based object — slot-0
// shape-id + `message`/`name`/`stack` slots), exactly like any user class.
//
// `.stack` is a REAL captured trace: `engine.trace_capture()` is the PRIVATE
// engine-internal global (arch/time/trace passthrough). Only prelude-origin code
// (this file) may name it — the engine's privacy gate denies user code.
//
// `toString()` follows JS `Error.prototype.toString`: `"<name>: <message>"`, or
// just `"<name>"` when the message is empty, or just `"<message>"` when the name
// is empty.
//
// Concatenated BEFORE the Map/Set stdlib in the merged prelude so the error
// SUBCLASSES (which `extends Error`) see the `Error` base already declared.

class Error {
  message: string;
  name: string;
  stack: string;
  constructor(message?: string) {
    this.message = message ?? "";
    this.name = "Error";
    this.stack = engine.trace_capture();
  }
  toString(): string {
    if (this.message === "") return this.name;
    if (this.name === "") return this.message;
    return this.name + ": " + this.message;
  }
}

class TypeError extends Error {
  constructor(message?: string) { super(message); this.name = "TypeError"; }
}

class RangeError extends Error {
  constructor(message?: string) { super(message); this.name = "RangeError"; }
}

class ReferenceError extends Error {
  constructor(message?: string) { super(message); this.name = "ReferenceError"; }
}

class SyntaxError extends Error {
  constructor(message?: string) { super(message); this.name = "SyntaxError"; }
}

class URIError extends Error {
  constructor(message?: string) { super(message); this.name = "URIError"; }
}

class EvalError extends Error {
  constructor(message?: string) { super(message); this.name = "EvalError"; }
}

class AggregateError extends Error {
  constructor(message?: string) { super(message); this.name = "AggregateError"; }
}
