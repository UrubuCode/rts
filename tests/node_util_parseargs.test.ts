// node:util — parseArgs.
import { describe, test, expect } from "rts:test";
import { parseArgs } from "node:util";

// boolean + string long options + positionals.
const cfg = {
    args: ["--verbose", "--name", "alice", "file1.txt", "file2.txt"],
    options: {
        verbose: { type: "boolean" },
        name: { type: "string" },
    },
    allowPositionals: true,
};
const r = parseArgs(cfg);
const verboseOk = r.values.verbose === true;
const nameOk = r.values.name === "alice";
const posOk = r.positionals.length === 2 && r.positionals[0] === "file1.txt";

// short options + --flag=value form.
const cfg2 = {
    args: ["-v", "--out=result.txt"],
    options: {
        verbose: { type: "boolean", short: "v" },
        out: { type: "string" },
    },
    allowPositionals: false,
};
const r2 = parseArgs(cfg2);
const shortOk = r2.values.verbose === true;
const inlineOk = r2.values.out === "result.txt";

describe("node:util parseArgs", () => {
    test("boolean flag", () => expect(verboseOk).toBe(true));
    test("string option", () => expect(nameOk).toBe(true));
    test("positionals", () => expect(posOk).toBe(true));
    test("short alias", () => expect(shortOk).toBe(true));
    test("--key=value form", () => expect(inlineOk).toBe(true));
});
