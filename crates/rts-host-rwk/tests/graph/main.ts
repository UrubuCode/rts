import { test, expect } from "rts:test";
import fallback, { answer, twice, visible } from "./dep.ts";
import * as everything from "./dep.ts";

test("a named export crosses", function () { expect(answer).toBe(42); });
test("an exported function crosses and runs", function () { expect(twice(21)).toBe(42); });
test("a renamed export crosses under its new name", function () { expect(visible).toBe(1); });
test("a default export crosses", function () { expect(fallback).toBe("the default"); });
test("the namespace holds them all", function () { expect(everything.answer).toBe(42); });
