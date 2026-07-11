// node:fs — fs.constants (access + copyfile flags).
import { describe, test, expect } from "rts:test";
import { constants } from "node:fs";
const fok = constants.F_OK;
const rok = constants.R_OK;
const wok = constants.W_OK;
const xok = constants.X_OK;
const cpx = constants.COPYFILE_EXCL;
describe("node:fs constants", () => {
    test("F_OK", () => expect(fok).toBe(0));
    test("R_OK", () => expect(rok).toBe(4));
    test("W_OK", () => expect(wok).toBe(2));
    test("X_OK", () => expect(xok).toBe(1));
    test("COPYFILE_EXCL", () => expect(cpx).toBe(1));
});
