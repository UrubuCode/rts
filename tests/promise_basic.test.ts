import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Promise.resolve atalho — nasce ja' fulfilled.
const p1 = promise.new_resolved(42);
print("state=" + promise.state(p1));   // 1 (fulfilled)
print("wait=" + promise.wait(p1));     // 42

// Promise.reject atalho.
const p2 = promise.new_rejected(7);
print("state=" + promise.state(p2));   // 2 (rejected)
print("try_value=" + promise.try_value(p2)); // 7

// Idempotencia: segundo resolve eh no-op.
const p3 = promise.new_pending();
print("primeiro_resolve=" + promise.resolve(p3, 1));  // 1
print("segundo_resolve=" + promise.resolve(p3, 2));   // 0
print("valor_final=" + promise.wait(p3));             // 1

// reject apos resolve tambem eh no-op (state ja' settled).
const p4 = promise.new_pending();
promise.resolve(p4, 100);
print("reject_apos_resolve=" + promise.reject(p4, 999));  // 0
print("valor_p4=" + promise.wait(p4));                    // 100

// state de handle invalido.
print("state_invalido=" + promise.state(0));  // -1

describe("promise primitive (issue #412)", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "state=1\nwait=42\nstate=2\ntry_value=7\nprimeiro_resolve=1\nsegundo_resolve=0\nvalor_final=1\nreject_apos_resolve=0\nvalor_p4=100\nstate_invalido=-1\n"
    );
  });
});
