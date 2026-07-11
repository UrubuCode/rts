// node:process — availableMemory / constrainedMemory.
import { describe, test, expect } from "rts:test";
import { availableMemory, constrainedMemory } from "node:process";

const avail = availableMemory();
const availOk = avail > 0; // some free RAM on any test host
const constrained = constrainedMemory();
const constrainedOk = constrained === 0; // unconstrained

describe("node:process memory", () => {
    test("availableMemory positive", () => expect(availOk).toBe(true));
    test("constrainedMemory unconstrained", () => expect(constrainedOk).toBe(true));
});
