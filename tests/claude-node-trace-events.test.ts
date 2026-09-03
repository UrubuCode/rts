// node:trace_events — category bookkeeping is real, no tracing sink exists
// anywhere in the engine (crates/rts-node/src/trace_events.rs's own `//!`).
// The specific, testable claim its doc makes (docs/reference/node/
// trace_events.md §4): the enabled-category set is reference-counted —
// two Tracing objects enabling the same category, one of them disabling it,
// leaves the category enabled. Tested in both directions below.

import { describe, test, expect } from "rts:test";
import { createTracing, getEnabledCategories } from "node:trace_events";

// --- basic construction ------------------------------------------------------
const t1 = createTracing({ categories: ["node", "v8"] });
const t1CategoriesOk = t1.categories === "node,v8";
const t1InitiallyDisabled = t1.enabled === false;

t1.enable();
const t1EnabledOk = t1.enabled === true;
const afterT1Enable = getEnabledCategories();
const afterT1EnableOk =
    afterT1Enable.indexOf("node") >= 0 && afterT1Enable.indexOf("v8") >= 0;

t1.disable();
const t1DisabledOk = t1.enabled === false;
const afterT1Disable = getEnabledCategories();
const afterT1DisableOk = afterT1Disable.indexOf("node") === -1 && afterT1Disable.indexOf("v8") === -1;

// A second enable() after a disable() re-enables (not a stuck no-op).
t1.enable();
const reenabledOk = getEnabledCategories().indexOf("node") >= 0;
t1.disable();

// A second call to enable() while already enabled does not double the
// refcount — one disable() from the SAME object is enough to clear it.
const t1b = createTracing({ categories: ["once"] });
t1b.enable();
t1b.enable(); // guarded no-op per the module's own doc
t1b.disable();
const noDoubleCountOk = getEnabledCategories().indexOf("once") === -1;

// --- reference counting across TWO Tracing objects --------------------------
// The behavior this module's doc calls out by name: category "shared",
// enabled by BOTH t2 and t3, stays enabled after only ONE of them disables.
const t2 = createTracing({ categories: ["shared"] });
const t3 = createTracing({ categories: ["shared"] });

t2.enable();
t3.enable();
const bothEnabledOk = getEnabledCategories().indexOf("shared") >= 0;

t2.disable();
// Still enabled: t3 never disabled its own reference.
const stillEnabledAfterOneDisableOk = getEnabledCategories().indexOf("shared") >= 0;

t3.disable();
// Now both references are gone — actually cleared.
const clearedAfterBothDisableOk = getEnabledCategories().indexOf("shared") === -1;

// --- the other direction: disable from the object that never enabled -------
// t4 enables "onlyme"; t5 (constructed with the SAME category, never
// enabled) calling disable() must not remove t4's live reference.
const t4 = createTracing({ categories: ["onlyme"] });
const t5 = createTracing({ categories: ["onlyme"] });
t4.enable();
t5.disable(); // t5 never enabled — its own module doc: disable() only
              // decrements categories THIS object itself turned on.
const survivesUnrelatedDisableOk = getEnabledCategories().indexOf("onlyme") >= 0;
t4.disable();
const finallyClearedOk = getEnabledCategories().indexOf("onlyme") === -1;

describe("node:trace_events — construction and single-object lifecycle", () => {
    test("createTracing() joins categories and starts disabled", () => {
        expect(t1CategoriesOk).toBe(true);
        expect(t1InitiallyDisabled).toBe(true);
    });
    test("enable() flips the flag and adds to getEnabledCategories()", () => {
        expect(t1EnabledOk).toBe(true);
        expect(afterT1EnableOk).toBe(true);
    });
    test("disable() flips the flag and removes from getEnabledCategories()", () => {
        expect(t1DisabledOk).toBe(true);
        expect(afterT1DisableOk).toBe(true);
    });
    test("re-enabling after a disable works", () => expect(reenabledOk).toBe(true));
    test("a second enable() while already enabled does not double the refcount", () =>
        expect(noDoubleCountOk).toBe(true));
});

describe("node:trace_events — the reference-counted set (spec doc §4)", () => {
    test("two objects enabling the same category: both show it enabled", () =>
        expect(bothEnabledOk).toBe(true));
    test("one of the two disabling leaves the category enabled", () =>
        expect(stillEnabledAfterOneDisableOk).toBe(true));
    test("the second one disabling finally clears it", () =>
        expect(clearedAfterBothDisableOk).toBe(true));
    test("disable() on an object that never enabled does not touch another's reference", () =>
        expect(survivesUnrelatedDisableOk).toBe(true));
    test("the object that actually enabled it can still clear it", () =>
        expect(finallyClearedOk).toBe(true));
});
