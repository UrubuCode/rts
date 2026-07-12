// node:fs globSync — filesystem glob matching (* / ? / [...] / **).

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { globSync, writeFileSync, mkdirSync, existsSync, rmSync } from "node:fs";

const dir = "/tmp/rts_glob_dir";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);
writeFileSync(dir + "/a.txt", "1");
writeFileSync(dir + "/b.txt", "2");
writeFileSync(dir + "/c.log", "3");

const txt = globSync(dir + "/*.txt");
const txtCount = txt.length;
const all = globSync(dir + "/*");
const allCount = all.length;
const none = globSync(dir + "/*.nomatch");
const noneCount = none.length;

rmSync(dir, { recursive: true, force: true });

describe("node:fs globSync", () => {
  test("*.txt matches exactly the two text files", () => {
    expect(txtCount).toBe(2);
  });
  test("* matches all three entries", () => {
    expect(allCount).toBe(3);
  });
  test("a non-matching pattern yields an empty list", () => {
    expect(noneCount).toBe(0);
  });
  test("matched paths are strings", () => {
    expect(typeof txt[0]).toBe("string");
  });
});
