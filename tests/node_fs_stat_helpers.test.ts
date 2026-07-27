import { describe, test, expect } from "rts:test";
import {
  writeFileSync,
  existsSync,
  statSync,
  rmSync,
} from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #287 / #453 — perguntas stat-like sobre um arquivo.
//
// Escrito originalmente com `isFileSync`/`isDirectorySync`/`sizeSync`, helpers
// so-do-RTS que foram drenados. O proprio comentario da versao anterior dizia que
// o `statSync` completo "fica para fase 2 quando os wrappers Stats estiverem
// prontos" — eles estao, entao o teste passa a usar a API do Node de verdade:
// `statSync(path)` devolve um `Stats` com `.isFile()`, `.isDirectory()` e `.size`.
//
// Path relativo a cwd: portavel em Linux/macOS/Windows. Caminhos hardcoded em
// `/tmp/...` so' funcionam em Unix; em Windows o cwd-drive nao tem `/tmp`.

writeFileSync("__rts_stat_helpers.txt", "hello");

const ex = existsSync("__rts_stat_helpers.txt");
print(ex ? "exists" : "no");

const st = statSync("__rts_stat_helpers.txt");

print(st.isFile() ? "file" : "no");
print(st.isDirectory() ? "dir" : "notdir");
print(`${st.size}`);

rmSync("__rts_stat_helpers.txt");

describe("fixture:node_fs_stat_helpers", () => {
  test("exists/isFile/isDirectory/size", () => {
    expect(__rtsCapturedOutput).toBe("exists\nfile\nnotdir\n5\n");
  });
});
