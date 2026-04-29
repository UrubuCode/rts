import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function ok(): Promise<string> { return "ok"; }
async function okNum(): Promise<number> { return 42; }

try {
  const r = await ok();
  print(r);
} catch (e) {
  print("should not reach (string)");
}

try {
  const n = await okNum();
  print(`${n}`);
} catch (e) {
  print("should not reach (number)");
}

describe("async_try_catch_no_throw", () => {
  test("catch nao executa quando nao ha throw", () =>
    expect(__rtsCapturedOutput).toBe("ok\n42\n"));
});
