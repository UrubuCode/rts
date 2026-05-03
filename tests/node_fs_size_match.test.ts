import { describe, test, expect } from "rts:test";
import { gc } from "rts";
import {
  writeFileSync,
  sizeSync,
  rmSync,
} from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #453 — sizeSync deve refletir o conteudo escrito por writeFileSync,
// inclusive em append-style overwrites de tamanhos diferentes.

const PATH = "__rts_size_match.txt";

writeFileSync(PATH, "hi");
const s1 = sizeSync(PATH);
const h1 = gc.string_from_i64(s1);
print(h1); gc.string_free(h1);

writeFileSync(PATH, "longer-content-here");
const s2 = sizeSync(PATH);
const h2 = gc.string_from_i64(s2);
print(h2); gc.string_free(h2);

writeFileSync(PATH, "");
const s3 = sizeSync(PATH);
const h3 = gc.string_from_i64(s3);
print(h3); gc.string_free(h3);

rmSync(PATH);

// sizeSync de arquivo inexistente: -1
const s4 = sizeSync(PATH);
const h4 = gc.string_from_i64(s4);
print(h4); gc.string_free(h4);

describe("fixture:node_fs_size_match", () => {
  test("size after write/overwrite/empty/missing", () => {
    expect(__rtsCapturedOutput).toBe("2\n19\n0\n-1\n");
  });
});
