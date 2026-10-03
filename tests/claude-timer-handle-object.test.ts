// A timer handle is a `Timeout`/`Immediate` OBJECT, not a bare number — the
// shape `@whiskeysockets/baileys` depends on when it writes
// `setTimeout(...).unref()`, measured against Node 22 before this file existed.
//
// It pins the behaviour and not the function: every assertion here is a
// sentence about what a program observes (`unref` answers the handle itself,
// `hasRef` tracks the flag and not the liveness, `clearTimeout` takes the
// object AND the primitive it coerces to), because the point of the change is
// that programs written against Node keep working.
//
// `time.sleep_ms` rather than `await`: a synchronous pause drains the loop, and
// this file has to observe a timer firing from top-level code — the same choice
// `set_timeout_interval.test.ts` records.
//
// NOT covered here, and it cannot be: that an `unref`ed timer does not hold the
// PROGRAM open. That is a statement about process exit, so it is measured by a
// standalone program rather than by a fixture whose process the harness owns.
import { describe, test, expect } from "rts:test";
import { time } from "rts";

const handle = setTimeout(() => {}, 1000);
const handleType = typeof handle;
const handleClass = handle.constructor.name;
const unrefAnswersItself = handle.unref() === handle;
const hasRefAfterUnref = handle.hasRef();
const refAnswersItself = handle.ref() === handle;
const hasRefAfterRef = handle.hasRef();
const refreshAnswersItself = handle.refresh() === handle;
const primitive = Number(handle);
const primitiveStable = Number(handle) === primitive;
clearTimeout(handle);

// A cleared handle is still a live object with a readable flag — Node's
// `hasRef` reports the ref FLAG, not whether the timer is still scheduled.
const survivesClear = handle.hasRef() === true;

// `clearTimeout` by the object.
let firedByObject = 0;
const byObject = setTimeout(() => { firedByObject = firedByObject + 1; }, 10);
clearTimeout(byObject);

// `clearTimeout` by the primitive the object coerces to — the cross-thread
// form Node supports and the reason `Symbol.toPrimitive` exists on `Timeout`.
let firedByNumber = 0;
const byNumber = setTimeout(() => { firedByNumber = firedByNumber + 1; }, 10);
clearTimeout(Number(byNumber));

// `Symbol.dispose` cancels, which is what `using t = setTimeout(...)` means.
let firedByDispose = 0;
const byDispose = setTimeout(() => { firedByDispose = firedByDispose + 1; }, 10);
byDispose[Symbol.dispose]();

time.sleep_ms(60);
const cancelledByObject = firedByObject;
const cancelledByNumber = firedByNumber;
const cancelledByDispose = firedByDispose;

// `refresh()` re-arms a timer that has already FIRED, from its original delay —
// the one thing a bare number could never express. Measured in Node 22: a
// refresh after the callback ran runs it a second time; a refresh after
// `clearTimeout` does NOT, because clearing destroys the timer. Both are
// asserted, because the pair is the whole of the rule.
let refreshed = 0;
const toRefresh = setTimeout(() => { refreshed = refreshed + 1; }, 10);
time.sleep_ms(40);
const beforeRefresh = refreshed;
toRefresh.refresh();
time.sleep_ms(60);
const afterRefresh = refreshed;

let notRefreshed = 0;
const cleared = setTimeout(() => { notRefreshed = notRefreshed + 1; }, 10);
clearTimeout(cleared);
cleared.refresh();
time.sleep_ms(60);
const clearedStaysDead = notRefreshed;

// An interval is a `Timeout` too, and clearing it by the object stops it.
let ticks = 0;
const interval = setInterval(() => { ticks = ticks + 1; }, 10);
const intervalClass = interval.constructor.name;
const intervalIsObject = typeof interval === "object";
time.sleep_ms(45);
clearInterval(interval);
const tickedThenStopped = ticks;
time.sleep_ms(40);
const stayedStopped = ticks === tickedThenStopped;

// An `Immediate` is a relative, not the same class: `ref`/`unref`/`hasRef` and
// `Symbol.dispose`, but no `refresh` and NO `Symbol.toPrimitive` — so
// `Number(immediate)` is NaN in Node, which this asserts rather than assumes.
let immediateFired = 0;
const immediate = setImmediate(() => { immediateFired = immediateFired + 1; });
const immediateClass = immediate.constructor.name;
const immediateHasUnref = typeof immediate.unref === "function";
const immediateHasRefresh = typeof (immediate as any).refresh;
const immediateNumber = Number(immediate);
clearImmediate(immediate);
time.sleep_ms(20);
const immediateCancelled = immediateFired;

// A foreign or already-cleared handle stays a quiet no-op, as before.
let crashed = false;
try {
    clearTimeout(0);
    clearTimeout(undefined as any);
    clearInterval("not-a-handle" as any);
    clearImmediate({} as any);
} catch (e) {
    crashed = true;
}

describe("timer handles are Timeout/Immediate objects", () => {
    test("setTimeout answers an object", () => expect(handleType).toBe("object"));
    test("its class is Timeout", () => expect(handleClass).toBe("Timeout"));
    test("unref answers the handle itself", () => expect(unrefAnswersItself).toBe(true));
    test("hasRef is false after unref", () => expect(hasRefAfterUnref).toBe(false));
    test("ref answers the handle itself", () => expect(refAnswersItself).toBe(true));
    test("hasRef is true again after ref", () => expect(hasRefAfterRef).toBe(true));
    test("refresh answers the handle itself", () => expect(refreshAnswersItself).toBe(true));
    test("the handle coerces to a number", () => expect(typeof primitive).toBe("number"));
    test("that number is stable between reads", () => expect(primitiveStable).toBe(true));
    test("a cleared handle is still readable", () => expect(survivesClear).toBe(true));

    test("clearTimeout takes the object", () => expect(cancelledByObject).toBe(0));
    test("clearTimeout takes the primitive", () => expect(cancelledByNumber).toBe(0));
    test("Symbol.dispose cancels", () => expect(cancelledByDispose).toBe(0));

    test("a timer fires once on its own", () => expect(beforeRefresh).toBe(1));
    test("refresh re-arms a timer that already fired", () => expect(afterRefresh).toBe(2));
    test("refresh does not revive a cleared timer", () => expect(clearedStaysDead).toBe(0));

    test("setInterval answers an object", () => expect(intervalIsObject).toBe(true));
    test("an interval's class is Timeout", () => expect(intervalClass).toBe("Timeout"));
    test("an interval ticked", () => expect(tickedThenStopped > 0).toBe(true));
    test("clearInterval by object stops it", () => expect(stayedStopped).toBe(true));

    test("setImmediate answers an Immediate", () => expect(immediateClass).toBe("Immediate"));
    test("an Immediate has unref", () => expect(immediateHasUnref).toBe(true));
    test("an Immediate has no refresh", () => expect(immediateHasRefresh).toBe("undefined"));
    test("an Immediate does not coerce to a number", () =>
        expect(Number.isNaN(immediateNumber)).toBe(true));
    test("clearImmediate by object cancels", () => expect(immediateCancelled).toBe(0));

    test("clearing junk stays quiet", () => expect(crashed).toBe(false));
});
