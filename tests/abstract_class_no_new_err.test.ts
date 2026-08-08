import { describe, test, expect } from "rts:test";
import { existsSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE_ABSTRACT_CLASS_NO_NEW_ERR = "// ERRO esperado: classe abstract não pode ser instanciada via new.\nabstract class Shape {\n    abstract area(): number;\n}\n\nconst s = new Shape();\n";

function resolveRtsExe(): string {
  // `fs.exists` answered 1/0; `existsSync` answers a boolean — same question.
  if (existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (existsSync("target/debug/rts")) return "target/debug/rts";
  if (existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("fixture:abstract_class_no_new_err", () => {
  test("fails with non-zero exit code", () => {
    writeFileSync("tests/__tmp_abstract_class_no_new_err.ts", SOURCE_ABSTRACT_CLASS_NO_NEW_ERR);
    const exe = resolveRtsExe();
    // `process.spawn(exe, "run\npath")` + `process.wait` collapse into one
    // `spawnSync` with a real argv; `.status` is the exit code `wait` returned.
    const code = spawnSync(exe, ["run", "tests/__tmp_abstract_class_no_new_err.ts"]).status;
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
