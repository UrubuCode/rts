import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_PRIVATE_METHOD_ERR = "// ERRO esperado: método private acessado de fora.\nclass C {\n    private secret(): number { return 42; }\n}\n\nconst c = new C();\nconst v = c.secret(); // erro\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:private_method_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_private_method_err.ts", SOURCE_PRIVATE_METHOD_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_private_method_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
