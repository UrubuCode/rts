import { describe, test, expect } from "rts:test";
import { existsSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE_ABSTRACT_MISSING_IMPL_ERR = "// ERRO esperado: classe concreta não implementa todos os abstract.\nabstract class Shape {\n    abstract area(): number;\n    abstract perimeter(): number;\n}\n\nclass Square extends Shape {\n    side: number = 5;\n    area(): number { return this.side * this.side; }\n    // perimeter() faltando!\n}\n\nconst s = new Square();\n";

function resolveRtsExe(): string {
  // `fs.exists` answered 1/0; `existsSync` answers a boolean — same question.
  if (existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (existsSync("target/debug/rts")) return "target/debug/rts";
  if (existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("fixture:abstract_missing_impl_err", () => {
  test("fails with non-zero exit code", () => {
    writeFileSync("tests/__tmp_abstract_missing_impl_err.ts", SOURCE_ABSTRACT_MISSING_IMPL_ERR);
    const exe = resolveRtsExe();
    // `process.spawn(exe, "run\npath")` + `process.wait` collapse into one
    // `spawnSync` with a real argv; `.status` is the exit code `wait` returned.
    const code = spawnSync(exe, ["run", "tests/__tmp_abstract_missing_impl_err.ts"]).status;
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
