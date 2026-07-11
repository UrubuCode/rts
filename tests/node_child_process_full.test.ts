// node:child_process — synchronous exec/spawn.
import { describe, test, expect } from "rts:test";
import { execSync, spawnSync, execFileSync } from "node:child_process";

// echo via the platform shell: `echo hello` prints "hello".
const out = execSync("echo hello");
const execOk = out.indexOf("hello") >= 0;

// spawnSync a program directly. Use the platform echo-ish: on Windows `cmd /c echo`.
// Portable: spawn the shell to run echo.
const isWin = out.indexOf("\r") >= 0 || true; // can't detect reliably; use cmd on Windows
let spawnStatus = -99;
let spawnStdout = "";
const r = spawnSync("cmd", ["/c", "echo", "world"]);
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
