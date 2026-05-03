import { describe, test, expect } from "rts:test";
import { gc } from "rts";
import {
  writeFileSync,
  existsSync,
  rmSync,
} from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #453 — writeFileSync deve ser sincrono: existsSync logo depois ve o
// arquivo. Cobre 2 variacoes: arquivo novo e sobrescrita.

const PATH = "__rts_write_exists.txt";

// Variacao A: cria do zero.
writeFileSync(PATH, "abc");
const a = existsSync(PATH);
const ah = gc.string_from_static(a ? "yes" : "no");
print(ah); gc.string_free(ah);

// Variacao B: sobrescreve. exists continua true.
writeFileSync(PATH, "xyz_long_content");
const b = existsSync(PATH);
const bh = gc.string_from_static(b ? "yes" : "no");
print(bh); gc.string_free(bh);

// Variacao C: depois de remover, exists vira false.
rmSync(PATH);
const c = existsSync(PATH);
const ch = gc.string_from_static(c ? "yes" : "no");
print(ch); gc.string_free(ch);

describe("fixture:node_fs_write_then_exists", () => {
  test("write+exists, overwrite, remove", () => {
    expect(__rtsCapturedOutput).toBe("yes\nyes\nno\n");
  });
});
