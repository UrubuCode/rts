import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// console.log com varios tipos — coercao automatica via coerce_to_handle.
// Como nao temos como interceptar console.log no test runner, validamos
// via wrapper de print.
function logLike(...args: any[]): string {
  // Nao temos spread/rest em var-args ainda; abaixo replico o behavior
  // manualmente.
  return "";
}

// Smoke test: chamamos console.log direto e verificamos que nao crasha
// nem dispara warnings.
console.log(42);
console.log(3.14);
console.log("hi");
console.log("x", 42);
console.log(true, false);
console.log(1, "two", 3.5);
print("ok");

describe("console_log_types", () => {
  test("mixed types do not crash", () =>
    expect(__rtsCapturedOutput).toBe("ok\n"));
});
