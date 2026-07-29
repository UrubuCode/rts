// node:child_process — synchronous exec/spawn.
import { describe, test, expect } from "rts:test";
import { execSync, spawnSync, execFileSync } from "node:child_process";
import { platform } from "node:os";

// echo via the platform shell: `echo hello` prints "hello".
const out = execSync("echo hello");
const execOk = out.indexOf("hello") >= 0;

// spawnSync a program directly, through the PLATFORM shell.
//
// This used to spawn `cmd /c echo world` unconditionally, guarded by
// `const isWin = out.indexOf("\r") >= 0 || true` — a detection that is always
// true by construction, with a comment admitting it "can't detect reliably".
// It can: `os.platform()`. On the ubuntu/macos runners there is no `cmd`, so the
// spawn failed and the assertion read `false`.
const isWin = platform() === "win32";
let spawnStatus = -99;
let spawnStdout = "";
const r = isWin
    ? spawnSync("cmd", ["/c", "echo", "world"])
    : spawnSync("sh", ["-c", "echo world"]);
spawnStatus = r.status;
spawnStdout = r.stdout;
const spawnOk = spawnStatus === 0 && spawnStdout.indexOf("world") >= 0;

// execSync of a failing command throws.
let failThrew = false;
try { execSync("exit 3"); } catch (e) { failThrew = true; }

describe("node:child_process", () => {
    test("execSync echo", () => expect(execOk).toBe(true));
    test("spawnSync status 0 + stdout", () => expect(spawnOk).toBe(true));
    test("execSync non-zero throws", () => expect(failThrew).toBe(true));
});
