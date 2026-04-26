import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_ABSTRACT_CLASS_NO_NEW_ERR = "// ERRO esperado: classe abstract não pode ser instanciada via new.\nabstract class Shape {\n    abstract area(): number;\n}\n\nconst s = new Shape();\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:abstract_class_no_new_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_abstract_class_no_new_err.ts", SOURCE_ABSTRACT_CLASS_NO_NEW_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_abstract_class_no_new_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
