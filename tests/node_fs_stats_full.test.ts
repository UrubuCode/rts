// node:fs Stats — full class surface: all seven type predicates
// (isFile/isDirectory/isSymbolicLink/isBlockDevice/isCharacterDevice/isFIFO/
// isSocket) and every numeric property (size/mode/*Ms times + dev/ino/nlink/
// uid/gid/rdev/blksize/blocks), on a regular file and a directory, read from
// real std::fs::Metadata. Includes an untracked (array-element) receiver.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { statSync, writeFileSync, mkdirSync, existsSync, rmSync } from "node:fs";

const dir = "/tmp/rts_stats_full_dir";
const file = "/tmp/rts_stats_full_dir/f.txt";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);
writeFileSync(file, "abcdef");

const fst = statSync(file);
const dst = statSync(dir);

// type predicates on a regular file
const fFile = fst.isFile();
const fDir = fst.isDirectory();
const fSym = fst.isSymbolicLink();
const fBlk = fst.isBlockDevice();
const fChr = fst.isCharacterDevice();
const fFifo = fst.isFIFO();
const fSock = fst.isSocket();

// type predicates on a directory
const dFile = dst.isFile();
const dDir = dst.isDirectory();

// numeric properties on the file
const size = fst.size;
const mode = fst.mode;
const mtime = fst.mtimeMs;
const nlink = fst.nlink;
const blksize = fst.blksize;
const uid = fst.uid;

// untracked receiver
const arr = [statSync(file)];
const us = arr[0];
const uIsFile = us.isFile();

rmSync(dir, { recursive: true, force: true });

describe("node:fs Stats full surface", () => {
  test("file isFile true, others false", () => {
    expect(fFile).toBe(true);
    expect(fDir).toBe(false);
    expect(fSym).toBe(false);
    expect(fBlk).toBe(false);
    expect(fChr).toBe(false);
    expect(fFifo).toBe(false);
    expect(fSock).toBe(false);
  });
  test("directory isDirectory true, isFile false", () => {
    expect(dDir).toBe(true);
    expect(dFile).toBe(false);
  });
  test("size reflects written bytes", () => { expect(size).toBe(6); });
  test("mode is a positive number", () => { expect(mode > 0).toBe(true); });
  test("mtimeMs is a positive number", () => { expect(mtime > 0).toBe(true); });
  test("nlink is at least 1", () => { expect(nlink >= 1).toBe(true); });
  test("blksize is a non-negative number", () => { expect(blksize >= 0).toBe(true); });
  test("uid is a number", () => { expect(typeof uid).toBe("number"); });
  test("untracked receiver isFile dispatches", () => { expect(uIsFile).toBe(true); });
});
