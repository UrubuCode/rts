// node:constants — the deprecated flattened view, and what it must NOT be.
//
// The assertions compare against `os.constants` and `fs.constants` rather than
// against literal numbers, and that is the point: this module is a VIEW, so the
// defect worth pinning is the two copies drifting apart, not any one value. A
// test written as `expect(constants.O_RDONLY).toBe(0)` would pass just as well
// against a second, independently typed table — which is the thing we do not
// want to have.
import { describe, test, expect } from "rts:test";
import * as constants from "node:constants";
import * as os from "node:os";
import * as fs from "node:fs";

const errnoName = Object.keys(os.constants.errno)[0];
const fsName = Object.keys(fs.constants)[0];

describe("node:constants", () => {
    test("it is not empty", () => expect(Object.keys(constants).length > 0).toBe(true));
    test("an fs constant agrees with node:fs", () =>
        expect(constants[fsName]).toBe(fs.constants[fsName]));
    test("an errno agrees with os.constants.errno", () =>
        expect(constants[errnoName]).toBe(os.constants.errno[errnoName]));
    test("the fs open flags are flattened, not nested", () =>
        expect(typeof constants.O_RDONLY).toBe("number"));
    test("priority is NOT spread in — Node does not", () =>
        expect(constants.PRIORITY_NORMAL).toBe(undefined));
});
