// node:fs/promises — the Promise-returning forms. The work is synchronous
// (interim event loop, #207), so a top-level awaited call resolves immediately
// and a failed op rejects (catchable with try/catch around await).

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import {
  readFile, writeFile, stat, readdir, access,
} from "node:fs/promises";
import { writeFileSync, existsSync, rmSync, mkdirSync } from "node:fs";

const file = "/tmp/rts_prom_read.txt";
if (existsSync(file)) { rmSync(file); }
writeFileSync(file, "promise read");

const file2 = "/tmp/rts_prom_write.txt";
if (existsSync(file2)) { rmSync(file2); }

const dir = "/tmp/rts_prom_dir";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);
writeFileSync(dir + "/e.txt", "1");

// Top-level await (the promises settle synchronously).
const readResult = await readFile(file, "utf8");
await writeFile(file2, "written via promise");
const writeThenReadBack = await readFile(file2, "utf8");
const st = await stat(file2);
const statIsFile = st.isFile();
const names = await readdir(dir);
const dirCount = names.length;
await access(file);

let errCaught = false;
let errCode = "none";
try {
  await readFile("/tmp/rts_prom_nope_xyz.txt", "utf8");
} catch (e) {
  errCaught = true;
  errCode = e.code;
}

rmSync(file);
rmSync(file2);
rmSync(dir, { recursive: true, force: true });

describe("node:fs/promises", () => {
  test("readFile resolves with contents", () => {
    expect(readResult).toBe("promise read");
  });
  test("writeFile then readFile round-trips", () => {
    expect(writeThenReadBack).toBe("written via promise");
  });
  test("stat resolves a working Stats", () => {
    expect(statIsFile).toBe(true);
  });
  test("readdir resolves the entry names", () => {
    expect(dirCount).toBe(1);
  });
  test("a failed op rejects (catchable) with ENOENT", () => {
    expect(errCaught).toBe(true);
    expect(errCode).toBe("ENOENT");
  });
});
