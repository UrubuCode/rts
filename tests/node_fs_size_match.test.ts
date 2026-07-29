import { describe, test, expect } from "rts:test";
import { writeFileSync, statSync, existsSync, rmSync } from "node:fs";

// The size reported by `statSync` must track what `writeFileSync` wrote,
// including an overwrite that SHRINKS the file (the case that regressed when a
// write truncated lazily and left the old length behind).
//
// Used to import a flat `sizeSync` from the pre-`node:` era, which no longer
// exists — `node:fs` publishes the real Node surface, where size comes off a
// `Stats`. The "missing file" case changed with it: `sizeSync` answered `-1`,
// while Node THROWS `ENOENT` from `statSync`. Asserted as the throw, since that
// is the behaviour the runtime now has and the one Node has.

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const PATH = "__rts_size_match.txt";

writeFileSync(PATH, "hi");
print(`${statSync(PATH).size}`);

writeFileSync(PATH, "longer-content-here");
print(`${statSync(PATH).size}`);

// Shrink back to empty — the interesting case.
writeFileSync(PATH, "");
print(`${statSync(PATH).size}`);

rmSync(PATH);
print(existsSync(PATH) ? "still-there" : "gone");

let threw = "no";
try {
  statSync(PATH);
} catch (e: any) {
  threw = "threw";
}
print(threw);

describe("fixture:node_fs_size_match", () => {
  test("size after write/overwrite/shrink, then removal", () => {
    expect(__rtsCapturedOutput).toBe("2\n19\n0\ngone\nthrew\n");
  });
});
