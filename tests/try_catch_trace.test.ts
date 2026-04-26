import { describe, test, expect } from "rts:test";
import { trace } from "rts";

let __out: string = "";
function print(v: string): void { __out += v + "\n"; }

// Throw with trace frames on the stack — the error slot captures them.
// After catch, verify we can still use trace normally.

describe("fixture:try_catch_trace", () => {
  test("trace stack survives try/catch", () => {
    trace.push_frame("app.ts", "riskyOp", 5, 3);

    let caught: string = "";
    try {
      throw "something went wrong";
    } catch (e) {
      caught = e;
    }

    // After catch, frame is still on the stack (no auto-pop on throw)
    const d = trace.depth();
    trace.pop_frame();

    expect(caught).toBe("something went wrong");
    expect(d).toBe(1);
    print("ok");
  });

  test("no frames: throw/catch still works", () => {
    let caught: string = "";
    try {
      throw "bare throw";
    } catch (e) {
      caught = e;
    }
    expect(caught).toBe("bare throw");
    print("ok");
  });

  test("nested try/catch with frames", () => {
    trace.push_frame("lib.ts", "outer", 10, 1);
    trace.push_frame("lib.ts", "inner", 20, 5);

    let result: string = "";
    try {
      try {
        throw "inner error";
      } catch (e) {
        result = "inner caught: " + e;
        throw "rethrown";
      }
    } catch (e2) {
      result = result + " | outer caught: " + e2;
    }

    trace.pop_frame();
    trace.pop_frame();

    expect(result).toBe("inner caught: inner error | outer caught: rethrown");
    print("ok");
  });
});
