// node:fs/promises — FileHandle (fs.promises.open). Object-backed handle over an
// fd; its methods return settled Promises. Awaited on an untracked receiver, they
// dispatch via the object-backed runtime dispatch.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { open } from "node:fs/promises";
import { writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";

const rfile = "/tmp/rts_fh_read.txt";
if (existsSync(rfile)) { rmSync(rfile); }
writeFileSync(rfile, "handle content");

const wfile = "/tmp/rts_fh_write.txt";
if (existsSync(wfile)) { rmSync(wfile); }

// read via a FileHandle
const rh = await open(rfile, "r");
const content = await rh.readFile("utf8");
await rh.close();

// write via a FileHandle, then stat + truncate through it
const wh = await open(wfile, "w");
await wh.writeFile("written by filehandle");
const st = await wh.stat();
const statIsFile = st.isFile();
await wh.sync();
await wh.close();
const readBack = readFileSync(wfile, "utf8");

rmSync(rfile);
rmSync(wfile);

describe("node:fs/promises FileHandle", () => {
  test("open + readFile reads the file contents", () => {
    expect(content).toBe("handle content");
  });
  test("open(w) + writeFile writes through the handle", () => {
    expect(readBack).toBe("written by filehandle");
  });
  test("handle.stat resolves a working Stats", () => {
    expect(statIsFile).toBe(true);
  });
});
