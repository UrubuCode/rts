// (#287) node:fs basic — readFileSync, writeFileSync, existsSync.
//
// The reads pass "utf8" explicitly. Without an encoding Node returns a Buffer
// and relies on `Buffer.prototype.toString()` to decode it; RTS documents that
// instance method as DEFERRED (crates/rts-node/src/buffer/mod.rs: it needs
// JsKind-level Buffer tracking in the front so a Buffer receiver stops resolving
// to Array.prototype.toString, which comma-joins the bytes). This file was
// reading with no encoding and comparing the result to a string, so it was
// asserting a semantic the runtime states it does not implement yet — and got
// "104,101,108,..." back. `readFileSync(p, "utf8")` is the Node-correct way to
// ask for text and is fully supported.

import { describe, test, expect } from "rts:test";
// `env.get_var` do namespace `rts` virou `process.env` de node:process — a
// mesma leitura de variavel de ambiente, na superficie que fica.
import process from "node:process";
import {
    readFileSync,
    writeFileSync,
    existsSync,
    appendFileSync,
} from "node:fs";

const tmp = process.env.TEMP || "/tmp";

const path = tmp + "/rts_node_fs_test.txt";
writeFileSync(path, "hello from rts");
const content: string = readFileSync(path, "utf8");
const exists = existsSync(path);

const path2 = tmp + "/rts_node_fs_missing.txt";
const exists2 = existsSync(path2);

// append
const appendPath = tmp + "/rts_node_fs_append.txt";
writeFileSync(appendPath, "line1\n");
appendFileSync(appendPath, "line2\n");
const appended: string = readFileSync(appendPath, "utf8");

// roundtrip large-ish
const big = "x".repeat(1000);
writeFileSync(tmp + "/rts_node_fs_big.txt", big);
const bigRead: string = readFileSync(tmp + "/rts_node_fs_big.txt", "utf8");
const bigLen: i64 = bigRead.length;

describe("node_fs_basic", () => {
    test("writeFileSync + readFileSync roundtrip", () =>
        expect(content).toBe("hello from rts"));
    test("existsSync true after write", () => expect(exists).toBe(true));
    test("existsSync false for missing", () => expect(exists2).toBe(false));
    test("appendFileSync appends to existing", () =>
        expect(appended).toBe("line1\nline2\n"));
    test("large content roundtrip length", () =>
        expect(bigLen).toBe(1000));
});
