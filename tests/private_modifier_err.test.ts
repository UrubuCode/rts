import { describe, test, expect } from "rts:test";
// `fs.*` e `process.*` do namespace `rts` foram trocados pela superficie que
// fica: node:fs e node:child_process. `process.spawn` + `process.wait` (handle
// e espera separada) viram um unico `spawnSync`, cujo `.status` e' o mesmo exit
// code — a assercao continua sendo "o compilador rejeita, exit != 0".
import { existsSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE_PRIVATE_MODIFIER_ERR = "// ERRO esperado: tentar acessar private de fora da classe.\nclass C {\n    private secret: number = 42;\n}\n\nconst c = new C();\nconst x = c.secret; // erro: secret é private\n";

function resolveRtsExe(): string {
  if (existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (existsSync("target/debug/rts")) return "target/debug/rts";
  if (existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("fixture:private_modifier_err", () => {
  test("fails with non-zero exit code", () => {
    writeFileSync("tests/__tmp_private_modifier_err.ts", SOURCE_PRIVATE_MODIFIER_ERR);
    const exe = resolveRtsExe();
    const child: any = spawnSync(exe, ["run", "tests/__tmp_private_modifier_err.ts"]);
    const code = child.status;
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
