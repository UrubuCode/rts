import { describe, test, expect } from "rts:test";
// `fs.*` e `process.*` do namespace `rts` foram trocados pela superficie que
// fica: node:fs e node:child_process. `process.spawn` + `process.wait` (handle
// e espera separada) viram um unico `spawnSync`, cujo `.status` e' o mesmo exit
// code — a assercao continua sendo "o compilador rejeita, exit != 0".
import { existsSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE_PRIVATE_FIELD_CROSS_CLASS_ERR = "// ERRO esperado: A não declara #x mas tenta acessar de instância de B.\nclass A {\n    foo(b: B): number { return b.#x; }\n}\nclass B {\n    #x: number = 5;\n}\nnew A().foo(new B());\n";

function resolveRtsExe(): string {
  if (existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (existsSync("target/debug/rts")) return "target/debug/rts";
  if (existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("fixture:private_field_cross_class_err", () => {
  test("fails with non-zero exit code", () => {
    writeFileSync("tests/__tmp_private_field_cross_class_err.ts", SOURCE_PRIVATE_FIELD_CROSS_CLASS_ERR);
    const exe = resolveRtsExe();
    const child: any = spawnSync(exe, ["run", "tests/__tmp_private_field_cross_class_err.ts"]);
    const code = child.status;
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
