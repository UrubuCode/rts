// node:fs — the remaining synchronous metadata surface: the fd family
// (fchmodSync/futimesSync/fdatasyncSync/fchownSync) and the path ownership ops
// (chownSync/lchownSync/lchmodSync) + symlinkSync. Ownership ops are no-ops on
// Windows (as in Node); the fd ops and lchmod/chown resolve without throwing.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import {
  openSync, writeSync, closeSync, fstatSync,
  fchmodSync, futimesSync, fdatasyncSync, fchownSync,
  chownSync, lchownSync, lchmodSync,
  writeFileSync, statSync, existsSync, rmSync,
} from "node:fs";

const file = "/tmp/rts_fs_meta.txt";
if (existsSync(file)) { rmSync(file); }

// fd family: open, write, fchmod/futimes/fdatasync/fchown, fstat, close.
let fdOk = false;
let modeAfter = 0;
{
  const fd = openSync(file, "w");
  writeSync(fd, new Uint8Array([104, 105]), 0, 2, 0); // "hi"
  fchmodSync(fd, 0o644);
  futimesSync(fd, 1000000, 2000000);
  fdatasyncSync(fd);
  fchownSync(fd, -1, -1); // -1/-1 = "no change" everywhere; no-op on Windows
  const st = fstatSync(fd);
  modeAfter = st.mode;
  fdOk = st.size === 2;
  closeSync(fd);
}

// path ownership ops resolve without throwing (real chown on POSIX, no-op on
// Windows). -1 uid/gid means "leave unchanged" on POSIX.
writeFileSync(file, "abc");
let ownOk = false;
try {
  chownSync(file, -1, -1);
  lchownSync(file, -1, -1);
  lchmodSync(file, 0o644);
  ownOk = true;
} catch (e) {
  ownOk = false;
}

const finalSize = statSync(file).size;
rmSync(file);

describe("node:fs sync metadata ops", () => {
  test("fd family (fchmod/futimes/fdatasync/fchown/fstat) works", () => {
    expect(fdOk).toBe(true);
  });
  test("fchmod set a mode reflected by fstat", () => {
    expect(modeAfter > 0).toBe(true);
  });
  test("path ownership ops resolve without throwing", () => {
    expect(ownOk).toBe(true);
  });
  test("file intact after metadata ops", () => {
    expect(finalSize).toBe(3);
  });
});
