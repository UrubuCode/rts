// `new RegExp` on a pattern it cannot read REFUSES, loudly — #2837.
//
// A JS string holding a lone surrogate has no Rust spelling: `to_rust` answers
// nothing rather than substituting `U+FFFD`, and `text/mod.rs` names `RegExp`
// as one of the three callers that need exactly that refusal. What `RegExp` did
// with it was answer `undefined` — the same answer it gives a value that is not
// a string at all — and from a CONSTRUCTOR that is not a quiet failure: `new`
// hands back the half-built `this`, so the program carries an object whose tag
// is `[object Object]`, whose `.source` throws "called on non-RegExp object",
// and which names neither the pattern nor the line that wrote it.
//
// Two different faults were answering the same way. This pins that they do not.
import { describe, test, expect } from "rts:test";

// Built at run time rather than written as a literal: the point is a STRING the
// engine cannot read, and a source file cannot hold an unpaired surrogate for
// the compiler to read either.
const lone = String.fromCharCode(0xd800);
const unreadable = "[" + lone + "]";

function attempt(make: () => any): { threw: boolean; name: string; message: string } {
    try { make(); return { threw: false, name: "", message: "" }; }
    catch (e: any) { return { threw: true, name: e.constructor.name, message: String(e.message) }; }
}

const constructed = attempt(() => new RegExp(unreadable));
const called = attempt(() => (RegExp as any)(unreadable));
// A value that is not a string at all keeps answering rather than throwing:
// `ToString` of an object calls user code an entry point cannot call, which is a
// stated gap and NOT the fault above. Pinned so the fix did not widen into it.
const notAString = attempt(() => new RegExp({} as any));
// And an ordinary bad pattern still refuses the way it always did, with the
// source quoted — which is what says the new arm did not swallow the old one.
const ordinary = attempt(() => new RegExp("("));

describe("a RegExp pattern with no Rust spelling (#2837)", () => {
    test("new RegExp refuses", () => expect(constructed.threw).toBe(true));
    test("and it is a SyntaxError", () => expect(constructed.name).toBe("SyntaxError"));
    test("the message says what is wrong", () =>
        expect(constructed.message.indexOf("unpaired surrogate") >= 0).toBe(true));
    test("the message does not try to quote the pattern", () =>
        expect(constructed.message.indexOf(lone) >= 0).toBe(false));
    test("RegExp without new refuses the same way", () => expect(called.name).toBe("SyntaxError"));
    test("a non-string pattern still answers instead", () => expect(notAString.threw).toBe(false));
    test("an ordinary bad pattern still refuses", () => expect(ordinary.name).toBe("SyntaxError"));
    test("and that one still quotes its source", () =>
        expect(ordinary.message.indexOf("/(/") >= 0).toBe(true));
});
