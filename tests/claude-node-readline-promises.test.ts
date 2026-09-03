// node:readline/promises — the parts that do NOT crash the process. See
// claude-node-readline-promises-crash.test.ts for the one call that does:
// EVERY call to this module's own createInterface() crashes, unconditionally
// — which is this module's entire reason to exist (Interface.question() as
// a Promise), so almost nothing of readline/promises is reachable from a
// test. This file is left with exactly the one thing that survives: the
// module-identity wiring `readline::namespace`'s own doc describes.
import { describe, test, expect } from "rts:test";
import { createInterface as createInterfacePromises } from "node:readline/promises";
import * as readline from "node:readline";

// `node:readline` carries `.promises` as a member (Node has since v17)
// alongside the separate `node:readline/promises` specifier, and the doc in
// readline/mod.rs says both are wired to the SAME object rather than two.
// Confirmed: importing only "node:readline/promises" (this file does not
// import "node:readline" for its createInterface) still reaches the exact
// same function identity as `require("readline").promises.createInterface`.
const sameModule = (readline as any).promises.createInterface === createInterfacePromises;

describe("node:readline/promises — module identity", () => {
    test("readline.promises.createInterface === (node:readline/promises).createInterface", () => {
        expect(sameModule).toBe(true);
    });
});
