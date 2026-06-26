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
  cause: any;
  // `options` is the ES2022 `{ cause }` bag; when present its `cause` becomes
  // `error.cause` (a non-object options is ignored, matching the spec leniency
  // for the corpus). The 1-arg form leaves `cause` undefined.
  constructor(message?: string, options?: any) {
    this.message = message ?? "";
    this.name = "Error";
    this.stack = engine.trace_capture();
    if (options !== undefined && options !== null) {
      this.cause = options.cause;
    }
  }
  toString(): string {
    if (this.message === "") return this.name;
    if (this.name === "") return this.message;
    return this.name + ": " + this.message;
  }
  // `Error.captureStackTrace(target, ctor?)` — V8/Node API. RTS já captura
  // `.stack` na construção (engine.trace_capture), então isto é um no-op. Existe
  // para o padrão comum `if (Error.captureStackTrace) Error.captureStackTrace(...)`
  // compilar e rodar (a leitura reifica este método como valor truthy; a chamada
  // não faz nada).
  static captureStackTrace(target?: any, ctor?: any): void {}
}

class TypeError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "TypeError"; }
}

class RangeError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "RangeError"; }
}

class ReferenceError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "ReferenceError"; }
}

class SyntaxError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "SyntaxError"; }
}

class URIError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "URIError"; }
}

class EvalError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "EvalError"; }
}

class AggregateError extends Error {
  constructor(message?: string, options?: any) { super(message, options); this.name = "AggregateError"; }
}
