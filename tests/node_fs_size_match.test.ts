import { describe, test, expect } from "rts:test";
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
print(`${s1}`);

writeFileSync(PATH, "longer-content-here");
const s2 = sizeSync(PATH);
print(`${s2}`);

writeFileSync(PATH, "");
const s3 = sizeSync(PATH);
print(`${s3}`);

rmSync(PATH);

// sizeSync de arquivo inexistente: -1
const s4 = sizeSync(PATH);
print(`${s4}`);

describe("fixture:node_fs_size_match", () => {
  test("size after write/overwrite/empty/missing", () => {
    expect(__rtsCapturedOutput).toBe("2\n19\n0\n-1\n");
  });
});
