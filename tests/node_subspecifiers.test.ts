// node:assert/strict and node:dns/promises — the specifiers, and the IDENTITY.
//
// What this pins is not that the functions work: they already did, one property
// read away, as `assert.strict.equal` and `dns.promises.lookup`. It is that the
// specifier resolves to the SAME object rather than to a second one built
// beside it — two would compare unequal, and would drift apart the first time
// one of them gained a member.
import { describe, test, expect } from "rts:test";
import * as assertions from "node:assert";
import * as strict from "node:assert/strict";
import * as dns from "node:dns";
import * as resolving from "node:dns/promises";
import * as mod from "node:module";

describe("node: sub-specifiers", () => {
    test("assert/strict resolves to assert.strict", () =>
        expect(strict.equal).toBe(assertions.strict.equal));
    test("assert/strict is the strict view", () => {
        let threw = false;
        try {
            strict.equal(1, "1");
        } catch (error) {
            threw = true;
        }
        expect(threw).toBe(true);
    });
    test("dns/promises resolves to dns.promises", () =>
        expect(resolving.lookup).toBe(dns.promises.lookup));
    test("dns/promises carries the lookup flags", () =>
        expect(resolving.ADDRCONFIG).toBe(4));
    test("isBuiltin knows assert/strict", () =>
        expect(mod.isBuiltin("assert/strict")).toBe(true));
    test("isBuiltin knows dns/promises", () =>
        expect(mod.isBuiltin("dns/promises")).toBe(true));
});
