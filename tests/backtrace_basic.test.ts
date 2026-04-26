import { describe, test, expect } from "rts:test";
import { io, gc, backtrace } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// backtrace.capture + to_string: nao podemos comparar conteudo
// (varia por build/OS), so validamos que capture retorna handle
// nao-zero e que to_string produz string GC.

const bt = backtrace.capture();
if (bt == 0) {
  print("FAIL: capture retornou 0");
} else {
  print("captured");
}

const s = backtrace.to_string(bt);
if (s == 0) {
  print("FAIL: to_string retornou 0");
} else {
  print("formatted");
  gc.string_free(s);
}

backtrace.free(bt);
print("freed");

describe("fixture:backtrace_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("captured\nformatted\nfreed\n");
  });
});
