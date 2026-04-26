import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_PRIVATE_MODIFIER_ERR = "// ERRO esperado: tentar acessar private de fora da classe.\nclass C {\n    private secret: number = 42;\n}\n\nconst c = new C();\nconst x = c.secret; // erro: secret é private\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:private_modifier_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_private_modifier_err.ts", SOURCE_PRIVATE_MODIFIER_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_private_modifier_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
