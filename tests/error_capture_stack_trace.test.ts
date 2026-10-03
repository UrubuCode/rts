import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Error.captureStackTrace (V8/Node). What this file pins is the GUARDED pattern
// libraries write — `if (Error.captureStackTrace) Error.captureStackTrace(this,
// Ctor)` — which must leave the construction of the error alone either way, so it
// held while the name was absent and holds now that it writes a real `.stack`.
// The entry's own contract is in tests/claude-error-capture-stack-trace.test.ts.
class AppError extends Error {
  constructor(message: string, public code: number) {
    super(message);
    this.name = "AppError";
    if (Error.captureStackTrace) Error.captureStackTrace(this, AppError);
  }
}

const e = new AppError("boom", 42);
print("name=" + e.name);
print("msg=" + e.message);
print("code=" + e.code);
print("isErr=" + (e instanceof Error));
print("isApp=" + (e instanceof AppError));

describe("Error.captureStackTrace stub", () => {
  test("padrao captureStackTrace nao afeta construcao", () =>
    expect(out).toBe("name=AppError\nmsg=boom\ncode=42\nisErr=true\nisApp=true\n"));
});
