import { describe, test, expect } from "rts:test";
import { io, backtrace } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// capture_if_enabled retorna 0 quando RUST_BACKTRACE nao esta set.
// Nos testes ele nao esta — checamos comportamento.

const bt = backtrace.capture_if_enabled();
if (bt == 0) {
  print("disabled");
} else {
  print("enabled");
  backtrace.free(bt);
}

describe("fixture:backtrace_disabled", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("disabled\n");
  });
});
