import { describe, test, expect } from "rts:test";
import { writeFileSync, readdirSync, mkdirSync, rmSync } from "node:fs";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const dir = "tmp_readdir_test_287";
mkdirSync(dir);
writeFileSync(dir + "/a.txt", "1");
writeFileSync(dir + "/b.txt", "2");

const entries = readdirSync(dir);
print(`len=${entries.length}`);

rmSync(dir + "/a.txt");
rmSync(dir + "/b.txt");

describe("nodespace_fs_readdir", () => {
  test("readdirSync returns entry names", () => {
    expect(__rtsCapturedOutput).toBe("len=2\n");
  });
});
