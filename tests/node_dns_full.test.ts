// node:dns — resolution via the OS resolver, results via callback.
import { describe, test, expect } from "rts:test";
import { lookup, resolve4 } from "node:dns";

let lookupAddr = "";
let lookupFamily = 0;
let lookupErr = true;
function onLookup(err: object, address: string, family: number) {
    lookupErr = err !== null;
    lookupAddr = address;
    lookupFamily = family;
}
lookup("localhost", onLookup);
const lookupOk = lookupErr === false && lookupAddr.length > 0 && (lookupFamily === 4 || lookupFamily === 6);

let r4: string[] = [];
let r4Err = true;
function onR4(err: object, addrs: string[]) {
    r4Err = err !== null;
    r4 = addrs;
}
resolve4("localhost", onR4);
const r4Ok = r4Err === false && r4.length >= 1;

let failErr: any = null;
function onFail(err: object, address: string) {
    failErr = err;
}
lookup("nonexistent.invalid.rts-test.example", onFail);
const failOk = failErr !== null && failErr.code === "ENOTFOUND";

describe("node:dns", () => {
    test("lookup localhost", () => expect(lookupOk).toBe(true));
    test("resolve4 localhost", () => expect(r4Ok).toBe(true));
    test("lookup failure err.code", () => expect(failOk).toBe(true));
});
