// node:fs — opendirSync/Dir (readSync cursor, path, closeSync) and the vectored
// fd I/O writevSync/readvSync. All synchronous, over real std::fs.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import {
  opendirSync, openSync, closeSync, writevSync, readvSync,
  writeFileSync, mkdirSync, existsSync, rmSync, readFileSync,
} from "node:fs";

// --- writevSync: two buffers concatenated to the file ---
const wfile = "/tmp/rts_writev.bin";
if (existsSync(wfile)) { rmSync(wfile); }
const wfd = openSync(wfile, "w");
const b1 = new Uint8Array([104, 105]);      // "hi"
const b2 = new Uint8Array([33]);            // "!"
const written = writevSync(wfd, [b1, b2]);
closeSync(wfd);
const wroteText = readFileSync(wfile, "utf8");

// --- readvSync: fill two buffers from the file ---
const rfd = openSync(wfile, "r");
const r1 = new Uint8Array([0, 0]);
const r2 = new Uint8Array([0]);
const readTotal = readvSync(rfd, [r1, r2]);
closeSync(rfd);
const r1a = r1[0], r1b = r1[1], r2a = r2[0];
rmSync(wfile);

// --- opendirSync / Dir ---
const dir = "/tmp/rts_opendir";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);
writeFileSync(dir + "/x.txt", "1");
writeFileSync(dir + "/y.txt", "2");
const d = opendirSync(dir);
const dPath = d.path;
let n = 0;
let e = d.readSync();
let firstIsFile = false;
if (e !== null) { firstIsFile = e.isFile(); }
while (e !== null) { n = n + 1; e = d.readSync(); }
d.closeSync();
rmSync(dir, { recursive: true, force: true });

describe("node:fs opendir/Dir + vectored IO", () => {
  test("writevSync writes all buffers", () => {
    expect(written).toBe(3);
    expect(wroteText).toBe("hi!");
  });
  test("readvSync fills buffers in order", () => {
    expect(readTotal).toBe(3);
    expect(r1a).toBe(104);
    expect(r1b).toBe(105);
    expect(r2a).toBe(33);
  });
  test("opendirSync path getter", () => {
    expect(dPath).toBe(dir);
  });
  test("Dir.readSync walks all entries then null", () => {
    expect(n).toBe(2);
  });
  test("Dir entries are Dirent (isFile dispatches)", () => {
    expect(firstIsFile).toBe(true);
  });
});
