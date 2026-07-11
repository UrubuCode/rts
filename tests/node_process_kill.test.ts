// node:process — kill with signal 0 (existence check, safe).
import { describe, test, expect } from "rts:test";
import { kill, pid } from "node:process";

// signal 0 on our own pid → the process exists → true.
const selfExists = kill(pid(), 0);
// a pid that almost certainly does not exist → false.
const bogus = kill(2147483000, 0);

describe("node:process kill", () => {
    test("self exists (signal 0)", () => expect(selfExists).toBe(true));
    test("bogus pid false", () => expect(bogus).toBe(false));
});
