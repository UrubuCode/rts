import { describe, test, expect } from "rts:test";
import { writeFileSync, existsSync, statSync, rmSync } from "node:fs";

// stat over `node:fs`, against the REAL Node surface.
//
// This file used to import `isFileSync` / `isDirectorySync` / `sizeSync` — flat
// helpers from the pre-`node:` era. Its own comment said a full `statSync`
// returning a `Stats` object was "fase 2, quando os wrappers estiverem
// prontos". That phase landed: `statSync` returns a real `Stats` with
// `.size` / `.isFile()` / `.isDirectory()`, and the flat helpers were dropped —
// so the test bailed at import. Rewritten onto `Stats`, same assertions.
//
// Path is relative to cwd on purpose: portable on Linux/macOS/Windows, unlike a
// hardcoded `/tmp/...` (on Windows the cwd drive has no `/tmp`).

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const PATH = "__rts_stat_helpers.txt";

writeFileSync(PATH, "hello");

print(existsSync(PATH) ? "exists" : "no");

const st = statSync(PATH);
print(st.isFile() ? "file" : "no");
print(st.isDirectory() ? "dir" : "notdir");
print(`${st.size}`);

rmSync(PATH);

describe("fixture:node_fs_stat_helpers", () => {
  test("exists/isFile/isDirectory/size", () => {
    expect(__rtsCapturedOutput).toBe("exists\nfile\nnotdir\n5\n");
  });
});
