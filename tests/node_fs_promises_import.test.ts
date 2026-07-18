// node:fs — the `promises` sub-namespace object reachable via
// `import { promises } from "node:fs"` (Node's `fs.promises`). It is the SAME
// API as `node:fs/promises`; the work is synchronous (interim event loop, #207),
// so an awaited call resolves immediately.
let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { promises, writeFileSync, existsSync, rmSync, mkdirSync } from "node:fs";

const dir = "./__fs_promises_import_tmp";
if (!existsSync(dir)) mkdirSync(dir);
const file = dir + "/probe.txt";
writeFileSync(file, "hello-fs-promises");

const readBack: string = await promises.readFile(file, "utf8");
const st: any = await promises.stat(file);
const isFile = st.isFile();

await promises.writeFile(dir + "/w.txt", "written-via-promises");
const wBack: string = await promises.readFile(dir + "/w.txt", "utf8");

const entries: string[] = await promises.readdir(dir);
const hasProbe = entries.indexOf("probe.txt") >= 0;

let rejected = false;
try {
  await promises.readFile(dir + "/does-not-exist.txt", "utf8");
} catch (e: any) {
  rejected = (e && e.code === "ENOENT") ? true : false;
}

rmSync(dir, { recursive: true, force: true });

describe("node:fs promises import binding", () => {
  test("promises.readFile round-trips", () => expect(readBack).toBe("hello-fs-promises"));
  test("promises.stat returns a Stats (isFile)", () => expect(isFile).toBe(true));
  test("promises.writeFile then readFile", () => expect(wBack).toBe("written-via-promises"));
  test("promises.readdir lists the dir", () => expect(hasProbe).toBe(true));
  test("a missing file rejects with ENOENT", () => expect(rejected).toBe(true));
});
