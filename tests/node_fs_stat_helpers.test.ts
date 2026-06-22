import { describe, test, expect } from "rts:test";
import {
  writeFileSync,
  existsSync,
  isFileSync,
  isDirectorySync,
  sizeSync,
  rmSync,
} from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #287 / #453 — helpers stat-like sobre rts::fs (statSync completo
// retornando objeto Stats fica para fase 2 quando builtin/buffer/Stats
// wrappers estiverem prontos).
//
// Path relativo a cwd: portavel em Linux/macOS/Windows. Caminhos
// hardcoded em `/tmp/...` so' funcionam em Unix; em Windows o cwd-drive
// nao tem `/tmp` por padrao.

writeFileSync("__rts_stat_helpers.txt", "hello");

const ex = existsSync("__rts_stat_helpers.txt");
print(ex ? "exists" : "no");

const isFile = isFileSync("__rts_stat_helpers.txt");
print(isFile ? "file" : "no");

const isDir = isDirectorySync("__rts_stat_helpers.txt");
print(isDir ? "dir" : "notdir");

const sz = sizeSync("__rts_stat_helpers.txt");
print(`${sz}`);

rmSync("__rts_stat_helpers.txt");

describe("fixture:node_fs_stat_helpers", () => {
  test("exists/isFile/isDirectory/size", () => {
    expect(__rtsCapturedOutput).toBe("exists\nfile\nnotdir\n5\n");
  });
});
