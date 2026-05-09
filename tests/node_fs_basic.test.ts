// (#287) node:fs basic — readFileSync, writeFileSync, existsSync.

import { describe, test, expect } from "rts:test";
import { env } from "rts";
import {
    readFileSync,
    writeFileSync,
    existsSync,
    appendFileSync,
} from "node:fs";

const tmp = env.get_var("TEMP") || "/tmp";

const path = tmp + "/rts_node_fs_test.txt";
writeFileSync(path, "hello from rts");
const content: string = readFileSync(path);
const exists = existsSync(path);

const path2 = tmp + "/rts_node_fs_missing.txt";
const exists2 = existsSync(path2);

// append
const appendPath = tmp + "/rts_node_fs_append.txt";
writeFileSync(appendPath, "line1\n");
appendFileSync(appendPath, "line2\n");
const appended: string = readFileSync(appendPath);

// roundtrip large-ish
const big = "x".repeat(1000);
writeFileSync(tmp + "/rts_node_fs_big.txt", big);
const bigRead: string = readFileSync(tmp + "/rts_node_fs_big.txt");
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
