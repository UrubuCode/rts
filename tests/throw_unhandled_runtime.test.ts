import { describe, test, expect } from "rts:test";
import * as fs from "node:fs";
import { spawnSync } from "node:child_process";

const SOURCE = "throw \"boom\";\n";
const TEMP_PATH = "tests/__tmp_unhandled_throw_runtime.ts";

// CI invoca `cargo run -- test` (build debug), entao debug e o canon.
// Localmente, se `target/debug/rts.exe` ficou stale apos mudancas em
// codegen/runtime, este teste pode reportar um exit code de bug antigo.
// Solucao: rebuild com `cargo build` antes de rodar localmente.
function resolveRtsExe(): string {
  if (fs.existsSync("target/debug/rts.exe")) return "target/debug/rts.exe";
  if (fs.existsSync("target/release/rts.exe")) return "target/release/rts.exe";
  if (fs.existsSync("target/debug/rts")) return "target/debug/rts";
  if (fs.existsSync("target/release/rts")) return "target/release/rts";
  return "rts";
}

describe("uncaught throw", () => {
  test("rts run returns non-zero exit code", () => {
    fs.writeFileSync(TEMP_PATH, SOURCE);
    const exe = resolveRtsExe();
    // process.spawn(exe, "run\npath") + process.wait: os args iam num
    // unico string separado por \n. spawnSync leva o array de verdade e
    // ja espera, entao spawn+wait viram uma chamada; o exit code sai em
    // .status em vez do retorno de wait.
    const code = spawnSync(exe, ["run", TEMP_PATH]).status;
    fs.unlinkSync(TEMP_PATH);
    const failed = code != 0;
    expect(failed ? "1" : "0").toBe("1");
  });
});
