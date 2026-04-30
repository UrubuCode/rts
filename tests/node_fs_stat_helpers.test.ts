import { describe, test, expect } from "rts:test";
import { gc } from "rts";
import {
  writeFileSync,
  existsSync,
  isFileSync,
  isDirectorySync,
  sizeSync,
} from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #287 — helpers stat-like sobre rts::fs (statSync completo retornando
// objeto Stats fica para fase 2 quando builtin/buffer/Stats wrappers
// estiverem prontos).

writeFileSync("/tmp/__rts_stat_helpers.txt", "hello");

const ex = existsSync("/tmp/__rts_stat_helpers.txt");
const eh = gc.string_from_static(ex ? "exists" : "no");
print(eh); gc.string_free(eh);

const isFile = isFileSync("/tmp/__rts_stat_helpers.txt");
const fh = gc.string_from_static(isFile ? "file" : "no");
print(fh); gc.string_free(fh);

const isDir = isDirectorySync("/tmp/__rts_stat_helpers.txt");
const dh = gc.string_from_static(isDir ? "dir" : "notdir");
print(dh); gc.string_free(dh);

const sz = sizeSync("/tmp/__rts_stat_helpers.txt");
const sh = gc.string_from_i64(sz);
print(sh); gc.string_free(sh);

describe("fixture:node_fs_stat_helpers", () => {
  test("exists/isFile/isDirectory/size", () => {
    expect(__rtsCapturedOutput).toBe("exists\nfile\nnotdir\n5\n");
  });
});
