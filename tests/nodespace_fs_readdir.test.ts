import { describe, test, expect } from "rts:test";
import {
  writeFileSync,
  readdirSync,
  mkdirSync,
  rmSync,
  existsSync,
} from "node:fs";

// `readdirSync` over a directory this test builds itself.
//
// The test used to create the directory and remove only the two FILES, leaving
// the directory behind — so the next run died at `mkdirSync` with EEXIST and the
// whole file scored zero. A test that only passes on a clean checkout is not
// passing. It now clears any leftover first and removes the directory at the
// end, so repeated runs are identical.

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const dir = "tmp_readdir_test_287";

// A leftover from an interrupted earlier run must not fail this one.
if (existsSync(dir)) {
  if (existsSync(dir + "/a.txt")) rmSync(dir + "/a.txt");
  if (existsSync(dir + "/b.txt")) rmSync(dir + "/b.txt");
  rmSync(dir, { recursive: true });
}

mkdirSync(dir);
writeFileSync(dir + "/a.txt", "1");
writeFileSync(dir + "/b.txt", "2");

const entries = readdirSync(dir);
print(`len=${entries.length}`);
print(`has_a=${entries.indexOf("a.txt") >= 0}`);
print(`has_b=${entries.indexOf("b.txt") >= 0}`);

rmSync(dir + "/a.txt");
rmSync(dir + "/b.txt");
rmSync(dir, { recursive: true });
print(`cleaned=${!existsSync(dir)}`);

describe("nodespace_fs_readdir", () => {
  test("readdirSync returns entry names", () => {
    expect(__rtsCapturedOutput).toBe(
      "len=2\nhas_a=true\nhas_b=true\ncleaned=true\n"
    );
  });
});
