import { describe, test, expect } from "rts:test";
import {
  writeFileSync,
  statSync,
  existsSync,
  rmSync,
} from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #453 — o tamanho reportado deve refletir o conteudo escrito por writeFileSync,
// inclusive em overwrites de tamanhos diferentes.
//
// Migrado de `sizeSync(path)` para `statSync(path).size`: `sizeSync` era uma API
// so-do-RTS, foi drenada, e o Node NAO a tem — o tamanho de um arquivo se le pelo
// `Stats`. O caso "arquivo inexistente" muda de forma junto: `sizeSync` devolvia
// -1, enquanto no Node `statSync` de um caminho ausente LANCA; a pergunta "ainda
// existe?" se faz com `existsSync`, que e como um programa real a escreve.

const PATH = "__rts_size_match.txt";

writeFileSync(PATH, "hi");
const s1 = statSync(PATH).size;
print(`${s1}`);

writeFileSync(PATH, "longer-content-here");
const s2 = statSync(PATH).size;
print(`${s2}`);

writeFileSync(PATH, "");
const s3 = statSync(PATH).size;
print(`${s3}`);

rmSync(PATH);

// removido: nao existe mais.
print(`${existsSync(PATH)}`);

describe("fixture:node_fs_size_match", () => {
  test("size after write/overwrite/empty, then gone", () => {
    expect(__rtsCapturedOutput).toBe("2\n19\n0\nfalse\n");
  });
});
