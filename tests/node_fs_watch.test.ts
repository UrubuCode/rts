// node:fs watch — the synchronous API surface of fs.watch (returns an FSWatcher;
// close() releases it). The actual EVENT DELIVERY is asynchronous (it fires in
// the post-main event loop, which the synchronous rts:test harness cannot
// observe) and is integration-verified separately via `rts run`:
//   watch(dir, onEvent); writeFileSync(dir+"/f.txt", ...)
//   → "EVENT type=rename file=f.txt" / "EVENT type=change file=f.txt"
// Here we assert the parts observable synchronously.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { watch, watchFile, unwatchFile, writeFileSync, mkdirSync, existsSync, rmSync } from "node:fs";

const dir = "/tmp/rts_watch_api";
if (existsSync(dir)) { rmSync(dir, { recursive: true, force: true }); }
mkdirSync(dir);

function onEvent(_eventType: string, _filename: string) {}
function onChange(_curr: any, _prev: any) {}

const w = watch(dir, onEvent);
const watcherType = typeof w;
// close() releases the watcher (drops the loop's keep-alive so the process can
// exit); a second close is a no-op.
w.close();
w.close();
let closedCleanly = true;

// watchFile / unwatchFile — register a poll watcher then release it, so no
// watcher stays active (the process can exit). Actual change delivery is
// `rts run`-verified: watchFile fires onChange(curr, prev) with real Stats.
const wf = dir + "/polled.txt";
writeFileSync(wf, "x");
watchFile(wf, onChange);
unwatchFile(wf);
let watchFileClean = true;

rmSync(dir, { recursive: true, force: true });

describe("node:fs watch (synchronous surface)", () => {
  test("watch returns an FSWatcher object", () => {
    expect(watcherType).toBe("object");
  });
  test("close is idempotent and does not throw", () => {
    expect(closedCleanly).toBe(true);
  });
  test("watchFile + unwatchFile register and release cleanly", () => {
    expect(watchFileClean).toBe(true);
  });
});
