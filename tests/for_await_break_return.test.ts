// `break` out of a loop must call the iterator's `return()`.
//
// THIS FILE IS EXPECTED TO FAIL on the async side today, and it is written to
// fail loudly rather than to be omitted: the language says an abrupt exit from
// `for`-`of` / `for await`-`of` closes the iterator, and a program that relies
// on it — a stream that must be destroyed, a file handle that must be closed, a
// lock that must be released — leaks the resource instead. Node calls `return()`
// on both forms; measured ten times, deterministic.
//
// It is its own file because the defect belongs to the EMITTER, not to any
// library: the iterables below are plain objects, with no `node:` module
// anywhere near them. It was found while pinning `for await` over a
// `node:stream` Readable, and pinning it there would have made a green
// `node:stream` file red for something no change to `node:stream` could fix.
import { describe, test, expect } from "rts:test";

let asyncReturned = false;
const asyncEndless = {
    [Symbol.asyncIterator]() {
        return {
            next() {
                return Promise.resolve({ value: 1, done: false });
            },
            return() {
                asyncReturned = true;
                return Promise.resolve({ value: undefined, done: true });
            },
        };
    },
};
for await (const value of asyncEndless) {
    break;
}

let syncReturned = false;
let syncCalls = 0;
const syncEndless = {
    [Symbol.iterator]() {
        return {
            next() {
                syncCalls = syncCalls + 1;
                return { value: syncCalls, done: false };
            },
            return() {
                syncReturned = true;
                return { value: undefined, done: true };
            },
        };
    },
};
for (const value of syncEndless) {
    break;
}

describe("break closes the iterator", () => {
    test("for-of break calls return()", () => expect(syncReturned).toBe(true));
    test("for-of break stops asking for values", () => expect(syncCalls).toBe(1));
    test("for await break calls return()", () => expect(asyncReturned).toBe(true));
});
