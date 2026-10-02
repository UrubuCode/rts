// node:events — `emit('error')` with no listener is a CATCHABLE throw, not the
// end of the process.
//
// Every expected value below was measured against real Node 22 before a line of
// the fix was written (`node` on PATH, one script per case, 2026-10-02):
//
//   emit("error", new Error("boom"))  -> throws THAT value; message "boom",
//                                       code undefined
//   with an "error" listener          -> does NOT throw, returns true, the
//                                       listener receives the error
//   emit("error", "uma string")       -> throws Error, name "Error",
//                                       code "ERR_UNHANDLED_ERROR",
//                                       message `Unhandled error. ('uma string')`,
//                                       .context === "uma string"
//   emit("error")                     -> same, message `Unhandled error. (undefined)`,
//                                       .context undefined
//   emit("error", 42)                 -> same, `Unhandled error. (42)`, .context 42
//   emit("tick", 1) with no listener  -> returns false, throws nothing
//
// What this pins is the THROW being catchable by the program. Before the fix,
// `emit` printed `rts: uncaught 'error' event: …` and called
// `std::process::exit(1)` from inside the native, so neither the `try`/`catch`
// around the call nor anything else in the program ever ran — the file died on
// its first case with no output at all. That is the behaviour this file refuses.
//
// Not asserted, and deliberately: the `Unhandled error. ({ a: 1 })` spelling for
// a non-Error OBJECT argument. Node renders that through `util.inspect`, which
// this engine does not reach from the raising native; the `code` and `.context`
// are what a program branches on and those ARE asserted for the object case.
import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";

/** What `emit` did: the value it threw, or the value it returned. */
function emitting(run: () => boolean): { threw?: any; returned?: boolean } {
    try {
        return { returned: run() };
    } catch (error: any) {
        return { threw: error };
    }
}

describe("node:events — 'error' with no listener throws instead of exiting", () => {
    test("an Error argument is thrown verbatim and the program keeps running", () => {
        const emitter = new EventEmitter();
        const thrown = new Error("boom");
        const outcome = emitting(() => emitter.emit("error", thrown));
        expect(outcome.threw).toBe(thrown);
        expect(outcome.threw.message).toBe("boom");
        expect(outcome.threw.code).toBe(undefined);
    });

    test("an 'error' listener makes emit answer true and throw nothing", () => {
        const emitter = new EventEmitter();
        let seen = "";
        emitter.on("error", (error: any) => {
            seen = error.message;
        });
        const outcome = emitting(() => emitter.emit("error", new Error("boom")));
        expect(outcome.returned).toBe(true);
        expect(seen).toBe("boom");
    });

    test("a string argument is wrapped in ERR_UNHANDLED_ERROR carrying .context", () => {
        const emitter = new EventEmitter();
        const outcome = emitting(() => emitter.emit("error", "uma string"));
        expect(outcome.threw instanceof Error).toBe(true);
        expect(outcome.threw.name).toBe("Error");
        expect(outcome.threw.code).toBe("ERR_UNHANDLED_ERROR");
        expect(outcome.threw.message).toBe("Unhandled error. ('uma string')");
        expect(outcome.threw.context).toBe("uma string");
    });

    test("no argument at all is ERR_UNHANDLED_ERROR with an undefined context", () => {
        const emitter = new EventEmitter();
        const outcome = emitting(() => emitter.emit("error"));
        expect(outcome.threw.code).toBe("ERR_UNHANDLED_ERROR");
        expect(outcome.threw.message).toBe("Unhandled error. (undefined)");
        expect(outcome.threw.context).toBe(undefined);
    });

    test("a number argument keeps its own spelling in the message", () => {
        const emitter = new EventEmitter();
        const outcome = emitting(() => emitter.emit("error", 42));
        expect(outcome.threw.code).toBe("ERR_UNHANDLED_ERROR");
        expect(outcome.threw.message).toBe("Unhandled error. (42)");
        expect(outcome.threw.context).toBe(42);
    });

    test("a non-Error object argument still arrives as .context", () => {
        const emitter = new EventEmitter();
        const bag = { a: 1 };
        const outcome = emitting(() => emitter.emit("error", bag));
        expect(outcome.threw.code).toBe("ERR_UNHANDLED_ERROR");
        expect(outcome.threw.context).toBe(bag);
    });

    test("an event that is not 'error' answers false and throws nothing", () => {
        const emitter = new EventEmitter();
        const outcome = emitting(() => emitter.emit("tick", 1));
        expect(outcome.returned).toBe(false);
    });

    test("the global EventEmitter, reached with no import, has the same contract", () => {
        const emitter = new (globalThis as any).EventEmitter();
        const thrown = new Error("global boom");
        const outcome = emitting(() => emitter.emit("error", thrown));
        expect(outcome.threw).toBe(thrown);
        const second = emitting(() => emitter.emit("error", "texto"));
        expect(second.threw.code).toBe("ERR_UNHANDLED_ERROR");
        expect(second.threw.context).toBe("texto");
    });
});
