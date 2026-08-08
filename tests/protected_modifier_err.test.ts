import { describe, test, expect } from "rts:test";
import * as fs from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE_PROTECTED_MODIFIER_ERR = "// ERRO esperado: protected acessado de fora da classe e fora de descendentes.\nclass Base {\n    protected y: number = 0;\n}\n\nconst b = new Base();\nconst v = b.y; // erro: protected em Base, escopo top-level\n";

function resolveRtsExe(): string {
  if (fs.existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (fs.existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (fs.existsSync("target/debug/rts")) return "target/debug/rts";
  if (fs.existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("fixture:protected_modifier_err", () => {
  test("fails with non-zero exit code", () => {
    fs.writeFileSync("tests/__tmp_protected_modifier_err.ts", SOURCE_PROTECTED_MODIFIER_ERR);
    const exe = resolveRtsExe();
    // process.spawn(exe, "run\npath") + process.wait: os args iam num
    // unico string separado por \n. spawnSync leva o array de verdade e
    // ja espera, entao spawn+wait viram uma chamada; o exit code sai em
    // .status em vez do retorno de wait.
    const code = spawnSync(exe, ["run", "tests/__tmp_protected_modifier_err.ts"]).status;
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
