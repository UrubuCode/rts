import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE = "throw \"boom\";\n";
const TEMP_PATH = "tests/__tmp_unhandled_throw_runtime.ts";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("uncaught throw", () => {
  test("rts run returns non-zero exit code", () => {
    fs.write(TEMP_PATH, SOURCE);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, `run\n${TEMP_PATH}`);
    const code = process.wait(child);
    fs.remove_file(TEMP_PATH);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
