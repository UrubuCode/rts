// node:crypto — unknown algorithm throws a catchable error (THROWS-flagged).
import { describe, test, expect } from "rts:test";
import { createHash, hash } from "node:crypto";

let createThrew = false;
try { createHash("not-a-real-algo"); } catch (e) { createThrew = true; }

let hashThrew = false;
try { hash("nope", "data"); } catch (e) { hashThrew = true; }

describe("node:crypto throws", () => {
    test("createHash unknown algo catchable", () => expect(createThrew).toBe(true));
    test("hash unknown algo catchable", () => expect(hashThrew).toBe(true));
});
