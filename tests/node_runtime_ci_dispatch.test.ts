// Runtime dispatch of object-backed Registry class instance methods on an
// UNTRACKED receiver (array element, function parameter) — previously threw
// "TypeError: <method> is not a function" because the front-end could not prove
// the receiver's class. Now resolved at runtime via the value's __rts_class tag.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { createHash } from "node:crypto";

// Pre-compute at top level (calling instance methods inside test() closures can
// hit GC; nested typed helpers hit a harness parse limit) — per test convention.
const ref = createHash("sha256").update("hello").digest("hex");

// Array-element receiver (untracked).
const arr = [createHash("sha256")];
const ae = arr[0];
ae.update("hello");
const arrElem = ae.digest("hex");

// Function-parameter receiver (untracked).
function digestOf(h: any): string {
  h.update("hello");
  return h.digest("hex");
}
const paramRes = digestOf(createHash("sha256"));

// digest() no-arg (Handle->Handle) on an untracked receiver.
const arr2 = [createHash("sha256")];
const be = arr2[0];
be.update("x");
const rawLen = be.digest().length;

describe("runtime class-instance dispatch (untracked receiver)", () => {
  test("array element receiver dispatches update/digest", () => {
    expect(arrElem).toBe(ref);
  });
  test("function parameter receiver dispatches", () => {
    expect(paramRes).toBe(ref);
  });
  test("digest() no-arg via untracked receiver returns 32 bytes", () => {
    expect(rawLen).toBe(32);
  });
});
