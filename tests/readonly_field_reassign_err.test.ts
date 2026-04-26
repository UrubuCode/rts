import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_READONLY_FIELD_REASSIGN_ERR = "// ERRO esperado: tentativa de reassign em readonly fora do ctor.\nclass C {\n    readonly x: number = 1;\n    set(v: number): void { this.x = v; }\n}\nconst c = new C();\nc.set(5);\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:readonly_field_reassign_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_readonly_field_reassign_err.ts", SOURCE_READONLY_FIELD_REASSIGN_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_readonly_field_reassign_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
