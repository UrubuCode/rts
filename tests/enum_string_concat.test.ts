import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// String enum em concat e template.

enum Logger {
  Info = "[INFO]",
  Warn = "[WARN]",
  Err = "[ERR]",
}

print(Logger.Info + " sistema iniciado");
print(`${Logger.Warn} memoria baixa`);
const tag: string = Logger.Err;
print(tag + " falhou");

describe("fixture:enum_string_concat", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("[INFO] sistema iniciado\n[WARN] memoria baixa\n[ERR] falhou\n");
  });
});
