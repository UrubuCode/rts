// node:fs — the callback (err-first) forms. The work is synchronous (interim
// event loop, #207), so the callback has run by the time the call returns and
// the captured results are readable at top level.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import {
  readFile, writeFile, stat, readdir, mkdir, unlink, exists, access,
  writeFileSync, existsSync, rmSync, mkdirSync,
} from "node:fs";

const file = "/tmp/rts_cb_read.txt";
if (existsSync(file)) { rmSync(file); }
writeFileSync(file, "hello cb");

// readFile(path, encoding, callback) → callback(err, string)
let readResult = "";
readFile(file, "utf8", (err, data) => { readResult = data; });

// writeFile(path, data, callback) → callback(err)
const file2 = "/tmp/rts_cb_write.txt";
if (existsSync(file2)) { rmSync(file2); }
let writeOk = false;
writeFile(file2, "written!", (err) => { writeOk = err === null; });
const wroteText = existsSync(file2) ? "exists" : "missing";

// stat(path, callback) → callback(err, Stats). (Reading a NUMERIC property off an
// untracked Stats — st.size — hits a separate getter-dispatch gap, tracked apart;
// the type predicates dispatch fine, proving the Stats was delivered.)
let statIsFile = false;
let statIsDir = false;
stat(file2, (err, st) => { statIsFile = st.isFile(); statIsDir = st.isDirectory(); });

// exists(path, callback) → callback(boolean)  [no err arg]
let existsRes = false;
exists(file, (ex) => { existsRes = ex; });

// error path: readFile of a missing file → callback(err) with err.code
let errCode = "none";
readFile("/tmp/rts_cb_nope_xyz.txt", "utf8", (err, data) => {
  if (err) { errCode = err.code; }
});

// readdir(path, callback) → callback(err, names)
const dir = "/tmp/rts_cb_dir";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);
writeFileSync(dir + "/one.txt", "1");
let dirCount = -1;
readdir(dir, (err, names) => { dirCount = names.length; });

rmSync(file);
rmSync(file2);
rmSync(dir, { recursive: true, force: true });

describe("node:fs callback forms", () => {
  test("readFile delivers the file contents", () => {
    expect(readResult).toBe("hello cb");
  });
  test("writeFile invokes callback with null err and writes", () => {
    expect(writeOk).toBe(true);
    expect(wroteText).toBe("exists");
  });
  test("stat delivers a working Stats object", () => {
    expect(statIsFile).toBe(true);
    expect(statIsDir).toBe(false);
  });
  test("exists callback receives a boolean", () => {
    expect(existsRes).toBe(true);
  });
  test("readFile of a missing file yields an ENOENT error", () => {
    expect(errCode).toBe("ENOENT");
  });
  test("readdir delivers the entry names", () => {
    expect(dirCount).toBe(1);
  });
});
