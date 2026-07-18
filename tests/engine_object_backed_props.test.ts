// Property WRITES on an OBJECT-BACKED Registry class instance.
//
// An object-backed class (a `Hash`, a `net.Server`, a `StringDecoder`) is an
// `Entry::Map` tagged `__rts_class`, not a shape-vec — so none of the engine's
// shape-slot write paths match it and, before this landed, `inst.foo = v`
// vanished silently. The instance map now stores the PolyValue word verbatim,
// which is what makes the write lossless for EVERY value kind rather than just
// the numbers a raw-i64 slot could hold.
//
// This is asserted across SEVERAL unrelated classes on purpose: the fix keys off
// the receiver's live Entry kind, never a class name, so it must hold for any
// object-backed class — the one in this test and the next one someone adds.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { createServer } from "node:net";
import { createHash } from "node:crypto";
import { StringDecoder } from "node:string_decoder";

// --- every value KIND round-trips ------------------------------------------
const server = createServer();
(server as any).aNumber = 42;
(server as any).aNegative = -7;
(server as any).aDouble = 1.5;
(server as any).aString = "hello";
(server as any).aBool = false;
(server as any).aNull = null;
(server as any).anUndefined = undefined;
(server as any).anObject = { nested: { deep: 9 } };
(server as any).anArray = [1, 2, 3];
(server as any).aFunction = () => 42;

const numberOut = (server as any).aNumber;
const negativeOut = (server as any).aNegative;
const doubleOut = (server as any).aDouble;
const stringOut = (server as any).aString;
const boolOut = (server as any).aBool;
const nullOut = (server as any).aNull;
const undefinedOut = (server as any).anUndefined;
const objectOut = (server as any).anObject.nested.deep;
const arrayOut = (server as any).anArray[1];
const arrayLen = (server as any).anArray.length;

const numberType = typeof (server as any).aNumber;
const doubleType = typeof (server as any).aDouble;
const stringType = typeof (server as any).aString;
const boolType = typeof (server as any).aBool;
const nullType = typeof (server as any).aNull;
const undefinedType = typeof (server as any).anUndefined;
const objectType = typeof (server as any).anObject;
const functionType = typeof (server as any).aFunction;

// A rewrite replaces the value AND may change its kind (the slot is tagged, so
// there is nothing to "convert" — it just holds the new word).
(server as any).aNumber = "now a string";
const rewrittenValue = (server as any).aNumber;
const rewrittenType = typeof (server as any).aNumber;

// An unwritten key is still `undefined` — a write stores a key, it does not
// invent the rest.
const neverWritten = (server as any).__never_written_here;

// --- the write does not disturb the class's own NATIVE surface --------------
// Native fields keep their raw encoding (`__rts_class` is a string handle, not
// a PolyValue word), so the class's real methods/getters must keep working
// after arbitrary JS properties are stored alongside them.
server.listen(0, "127.0.0.1");
const stillListening = server.listening;
const stillHasPort = server.address().port > 0;
server.close();

// --- the SAME fix, on unrelated object-backed classes -----------------------
const hash = createHash("sha256");
(hash as any).label = "checksum";
const hashLabel = (hash as any).label;
// The native surface still works after the write, and still hashes correctly.
const hashDigest = createHash("sha256").update("abc").digest("hex");
hash.update("abc");
const taggedDigest = hash.digest("hex");

const decoder = new StringDecoder("utf8");
(decoder as any).seen = 0;
(decoder as any).seen = (decoder as any).seen + 1;
const decoderSeen = (decoder as any).seen;
const decoderWorks = decoder.write(new Uint8Array([0x61, 0x62]) as any);

describe("object-backed instance: property writes round-trip", () => {
  test("integers, doubles and negatives keep their value and type", () => {
    expect(numberOut).toBe(42);
    expect(negativeOut).toBe(-7);
    expect(doubleOut).toBe(1.5);
    expect(numberType).toBe("number");
    expect(doubleType).toBe("number");
  });
  test("strings round-trip", () => {
    expect(stringOut).toBe("hello");
    expect(stringType).toBe("string");
  });
  test("a boolean stays a boolean — it does not decay to a number", () => {
    expect(boolOut).toBe(false);
    expect(boolType).toBe("boolean");
  });
  test("null and undefined are distinguishable", () => {
    expect(nullOut).toBe(null);
    expect(nullType).toBe("object");
    expect(undefinedOut).toBe(undefined);
    expect(undefinedType).toBe("undefined");
  });
  test("objects, arrays and functions round-trip by reference", () => {
    expect(objectOut).toBe(9);
    expect(objectType).toBe("object");
    expect(arrayOut).toBe(2);
    expect(arrayLen).toBe(3);
    expect(functionType).toBe("function");
  });
  test("a rewrite replaces the value and may change its kind", () => {
    expect(rewrittenValue).toBe("now a string");
    expect(rewrittenType).toBe("string");
  });
  test("an unwritten key is still undefined", () => {
    expect(neverWritten).toBe(undefined);
  });
});

describe("object-backed instance: the native surface is undisturbed", () => {
  test("a net.Server still binds and reports after JS props are stored", () => {
    expect(stillListening).toBe(true);
    expect(stillHasPort).toBe(true);
  });
  test("a crypto Hash still digests correctly after a JS prop is stored", () => {
    expect(hashLabel).toBe("checksum");
    expect(taggedDigest).toBe(hashDigest);
    expect(taggedDigest.length).toBe(64);
  });
  test("a StringDecoder still decodes, and its JS prop increments", () => {
    expect(decoderSeen).toBe(1);
    expect(decoderWorks).toBe("ab");
  });
});
