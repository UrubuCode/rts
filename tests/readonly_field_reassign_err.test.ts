import { describe, test, expect } from "rts:test";
import * as fs from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE_READONLY_FIELD_REASSIGN_ERR = "// ERRO esperado: tentativa de reassign em readonly fora do ctor.\nclass C {\n    readonly x: number = 1;\n    set(v: number): void { this.x = v; }\n}\nconst c = new C();\nc.set(5);\n";

function resolveRtsExe(): string {
  if (fs.existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (fs.existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (fs.existsSync("target/debug/rts")) return "target/debug/rts";
  if (fs.existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("fixture:readonly_field_reassign_err", () => {
  test("fails with non-zero exit code", () => {
    fs.writeFileSync("tests/__tmp_readonly_field_reassign_err.ts", SOURCE_READONLY_FIELD_REASSIGN_ERR);
    const exe = resolveRtsExe();
    // process.spawn(exe, "run\npath") + process.wait: os args iam num
    // unico string separado por \n. spawnSync leva o array de verdade e
    // ja espera, entao spawn+wait viram uma chamada; o exit code sai em
    // .status em vez do retorno de wait.
    const code = spawnSync(exe, ["run", "tests/__tmp_readonly_field_reassign_err.ts"]).status;
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
