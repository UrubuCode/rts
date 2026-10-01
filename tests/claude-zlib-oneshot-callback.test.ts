// A `node:zlib` one-shot REQUIRES its callback, and refuses loudly without one.
//
// It used to answer `undefined` quietly — the same answer it gives for a value
// that is not a buffer — so a program could ask for a compression, get nothing,
// and carry on. Node raises
// `TypeError [ERR_INVALID_ARG_TYPE]: The "callback" argument must be of type
// function. Received undefined`.
//
// Found as a CONTROL case: it was in a probe written to prove that guarding the
// optional callbacks of `node:http` (#2841) had not swallowed a refusal that
// should stay. It had not — this one was already missing before that change.
import { describe, test, expect } from "rts:test";
import { gzip, gunzip, deflate, inflate, gzipSync } from "node:zlib";

const input = Buffer.from("the quick brown fox");

function refusal(f: () => void): string {
    try { f(); return "did not throw"; } catch (e: any) { return e.constructor.name + ": " + String(e.message); }
}

const noCallback = refusal(() => (gzip as any)(input));
const deflateNoCallback = refusal(() => (deflate as any)(input));
const withOptionsNoCallback = refusal(() => (gzip as any)(input, { level: 6 }));

// The two-argument spelling — the callback where the options go — is the
// commonest form in the language and must still work. Asking for the callback
// before that collapse would refuse it.
let twoArgRan = false;
let twoArgBytes = 0;
gzip(input, (error: any, result: any) => { twoArgRan = error === null && result !== undefined; twoArgBytes = result ? result.length : 0; });

// And the three-argument form, options included.
let threeArgRan = false;
gzip(input, { level: 6 }, (error: any, result: any) => { threeArgRan = error === null && result !== undefined; });

// A round trip, so the refusal was not bought by breaking the work.
let roundTripped = "";
gzip(input, (_e: any, packed: any) => {
    gunzip(packed, (_e2: any, back: any) => { roundTripped = back.toString(); });
});

// The sync form takes no callback at all and must not have gained a demand for one.
const syncBytes = gzipSync(input).length;

// A buffer that is not one is still refused, and still names its own argument —
// so the new refusal did not take over the old one's message.
const notABuffer = refusal(() => (inflate as any)(42, () => {}));

describe("node:zlib one-shot requires its callback (#2843)", () => {
    test("gzip without a callback throws", () =>
        expect(noCallback.indexOf("did not throw")).toBe(-1));
    test("and it is a TypeError", () => expect(noCallback.indexOf("TypeError") === 0).toBe(true));
    test("naming the callback argument", () =>
        expect(noCallback.indexOf("callback") > 0).toBe(true));
    test("deflate refuses the same way", () => expect(deflateNoCallback.indexOf("TypeError") === 0).toBe(true));
    test("options but no callback still refuses", () =>
        expect(withOptionsNoCallback.indexOf("TypeError") === 0).toBe(true));
    test("the two-argument form still works", () => expect(twoArgRan).toBe(true));
    test("and produced bytes", () => expect(twoArgBytes > 0).toBe(true));
    test("the three-argument form still works", () => expect(threeArgRan).toBe(true));
    test("a round trip still comes back", () => expect(roundTripped).toBe("the quick brown fox"));
    test("the sync form needs no callback", () => expect(syncBytes > 0).toBe(true));
    test("a bad buffer is still refused for being a bad buffer", () =>
        expect(notABuffer.indexOf("callback")).toBe(-1));
});
