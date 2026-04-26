import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_PROTECTED_MODIFIER_ERR = "// ERRO esperado: protected acessado de fora da classe e fora de descendentes.\nclass Base {\n    protected y: number = 0;\n}\n\nconst b = new Base();\nconst v = b.y; // erro: protected em Base, escopo top-level\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:protected_modifier_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_protected_modifier_err.ts", SOURCE_PROTECTED_MODIFIER_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_protected_modifier_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
