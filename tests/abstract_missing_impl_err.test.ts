import { describe, test, expect } from "rts:test";
import { process, fs } from "rts";

const SOURCE_ABSTRACT_MISSING_IMPL_ERR = "// ERRO esperado: classe concreta não implementa todos os abstract.\nabstract class Shape {\n    abstract area(): number;\n    abstract perimeter(): number;\n}\n\nclass Square extends Shape {\n    side: number = 5;\n    area(): number { return this.side * this.side; }\n    // perimeter() faltando!\n}\n\nconst s = new Square();\n";

function resolveRtsExe(): string {
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  return "rts";
}

describe("fixture:abstract_missing_impl_err", () => {
  test("fails with non-zero exit code", () => {
    fs.write("tests/__tmp_abstract_missing_impl_err.ts", SOURCE_ABSTRACT_MISSING_IMPL_ERR);
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/__tmp_abstract_missing_impl_err.ts");
    const code = process.wait(child);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
