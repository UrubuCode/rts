// node:assert — match / doesNotMatch regex assertions.
import { describe, test, expect } from "rts:test";
import { match, doesNotMatch } from "node:assert";

let matchPass = false;
try { match("I will pass", /pass/); matchPass = true; } catch (e) {}

let matchFail = false;
try { match("nope", /pass/); } catch (e) { matchFail = true; }

let dnmPass = false;
try { doesNotMatch("hello", /world/); dnmPass = true; } catch (e) {}

let dnmFail = false;
try { doesNotMatch("hello world", /world/); } catch (e) { dnmFail = true; }

describe("node:assert match", () => {
    test("match passes", () => expect(matchPass).toBe(true));
    test("match throws on mismatch", () => expect(matchFail).toBe(true));
    test("doesNotMatch passes", () => expect(dnmPass).toBe(true));
    test("doesNotMatch throws on match", () => expect(dnmFail).toBe(true));
});
