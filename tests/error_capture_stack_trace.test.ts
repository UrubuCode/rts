import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Error.captureStackTrace (V8/Node) — no-op em RTS. O padrao comum em libs
// (`if (Error.captureStackTrace) Error.captureStackTrace(this, Ctor)`) deve
// compilar e rodar sem afetar a construcao do erro.
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
