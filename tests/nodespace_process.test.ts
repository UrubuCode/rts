import { describe, test, expect } from "rts:test";
import { pid, cwd, platform, arch } from "node:process";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const p = pid();
print(p > 0 ? "pid_ok" : "pid_fail");

const c = cwd();
print(c.length > 0 ? "cwd_ok" : "cwd_fail");

const plat = platform();
print(plat.length > 0 ? "platform_ok" : "platform_fail");

const a = arch();
print(a.length > 0 ? "arch_ok" : "arch_fail");

describe("nodespace_process", () => {
  test("pid/cwd/platform/arch return non-empty", () => {
    expect(__rtsCapturedOutput).toBe("pid_ok\ncwd_ok\nplatform_ok\narch_ok\n");
  });
});
