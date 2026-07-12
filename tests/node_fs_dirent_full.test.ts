// node:fs Dirent — readdirSync(path, { withFileTypes: true }) returns a Dirent[]
// whose entries carry name/parentPath and the seven type predicates, dispatched
// on array-element receivers (the object-backed runtime dispatch). The plain
// readdirSync(path) and { withFileTypes: false } still return string[].

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { readdirSync, writeFileSync, mkdirSync, existsSync, rmSync } from "node:fs";

const dir = "/tmp/rts_dirent_dir";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);
writeFileSync(dir + "/a.txt", "x");
mkdirSync(dir + "/sub");

// string[] forms
const plain = readdirSync(dir);
const plainLen = plain.length;
const plainType = typeof plain[0];
const noTypes = readdirSync(dir, { withFileTypes: false });
const noTypesType = typeof noTypes[0];

// Dirent[] form — index-based (readdir order is filesystem-dependent, so assert
// order-independent invariants: exactly one file, one directory, both names).
const ents = readdirSync(dir, { withFileTypes: true });
const entCount = ents.length;
const n0 = ents[0].name;
const f0 = ents[0].isFile();
const d0 = ents[0].isDirectory();
const p0 = ents[0].parentPath;
const n1 = ents[1].name;
const f1 = ents[1].isFile();
const d1 = ents[1].isDirectory();
const names = [n0, n1].slice().sort();
const fileCount = (f0 ? 1 : 0) + (f1 ? 1 : 0);
const dirCount = (d0 ? 1 : 0) + (d1 ? 1 : 0);

rmSync(dir, { recursive: true, force: true });

describe("node:fs Dirent (readdirSync withFileTypes)", () => {
  test("plain readdirSync returns string names", () => {
    expect(plainLen).toBe(2);
    expect(plainType).toBe("string");
  });
  test("withFileTypes:false still returns string[]", () => {
    expect(noTypes.length).toBe(2);
    expect(noTypesType).toBe("string");
  });
  test("withFileTypes:true returns two Dirent entries", () => {
    expect(entCount).toBe(2);
  });
  test("entries carry both names", () => {
    expect(names[0]).toBe("a.txt");
    expect(names[1]).toBe("sub");
  });
  test("exactly one file and one directory", () => {
    expect(fileCount).toBe(1);
    expect(dirCount).toBe(1);
  });
  test("parentPath is the scanned directory", () => {
    expect(p0).toBe(dir);
  });
});
