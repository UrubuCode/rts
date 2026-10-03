import { describe, test, expect } from "rts:test";

// `Symbol.species` answers the receiver on every built-in the specification gives
// one, and the three families that were missing it are the subject here: the
// eleven typed arrays, `ArrayBuffer` and `SharedArrayBuffer`.
//
// What this pins is not a descriptor for its own sake. `ws` — the WebSocket
// package a Node WhatsApp or Socket.IO client reaches for — opens
// `lib/receiver.js` with `const FastBuffer = Buffer[Symbol.species]` and then
// builds every RECEIVED frame with
// `new FastBuffer(buf.buffer, buf.byteOffset, n)`. With the hook absent that
// constant was `undefined`, so a socket connected, sent its first message, and
// died with `undefined is not a constructor` on the first byte the server
// answered — nothing wrong with the send path, and no frame in the stack to say
// so. That is what `@whiskeysockets/baileys` hit right after it logged
// `connected to WA`.
//
// `Buffer` reaches its answer by INHERITANCE, since `Object.getPrototypeOf(Buffer)`
// is `Uint8Array` here as in Node, so the Buffer section proves the inherited
// path rather than a second install.
//
// Every value below was measured against Node 22.23.2, and two places where
// Node's shape is deliberately NOT asserted, because it was measured and differs:
//
//   * `Buffer[Symbol.species]` is `FastBuffer` in Node — an internal subclass of
//     `Uint8Array` not reachable by name — and `Buffer` here. Both are
//     constructors that build a `Buffer`-compatible VIEW over an existing
//     `ArrayBuffer`, which is the whole of what `ws` asks of it, so this file
//     asserts the USE and not the identity. `=== Buffer` would have been a test
//     that passes here and fails in the runtime being matched.
//   * `Object.getOwnPropertyDescriptor(Uint8Array, Symbol.species)` is
//     `undefined` in Node, because the accessor sits once on the `%TypedArray%`
//     intrinsic and the eleven inherit it. This engine has no `%TypedArray%`
//     CONSTRUCTOR — `Object.getPrototypeOf(Uint8Array)` is `Function.prototype`
//     — so the accessor is installed per class and the descriptor is own. The
//     walk below reads it from whichever holder carries it, which is true in
//     both.
//
// No network and no `node_modules`: the read `ws` performs is three property
// accesses, and they are the whole defect.

// ── Pre-computed at the top, per CLAUDE.md's note about calling instance
// methods inside `test()`. ───────────────────────────────────────────────────

const speciesOf = (c: any) => c[Symbol.species];

const int8 = speciesOf(Int8Array) === Int8Array;
const uint8 = speciesOf(Uint8Array) === Uint8Array;
const uint8c = speciesOf(Uint8ClampedArray) === Uint8ClampedArray;
const int16 = speciesOf(Int16Array) === Int16Array;
const uint16 = speciesOf(Uint16Array) === Uint16Array;
const int32 = speciesOf(Int32Array) === Int32Array;
const uint32 = speciesOf(Uint32Array) === Uint32Array;
const f32 = speciesOf(Float32Array) === Float32Array;
const f64 = speciesOf(Float64Array) === Float64Array;
const bi64 = speciesOf(BigInt64Array) === BigInt64Array;
const bu64 = speciesOf(BigUint64Array) === BigUint64Array;

const ab = speciesOf(ArrayBuffer) === ArrayBuffer;
const sab = speciesOf(SharedArrayBuffer) === SharedArrayBuffer;
// `DataView` has NO species in the specification, and Node answers `undefined`
// for it too. Pinned so a later change installing the hook on every class in
// `buffers/` is caught rather than welcomed.
const dv = speciesOf(DataView);

const BufferCtor: any = (globalThis as any).Buffer;
const bufferParent = Object.getPrototypeOf(BufferCtor) === Uint8Array;
const bufferSpeciesKind = typeof BufferCtor[Symbol.species];

// The three lines of `ws/lib/receiver.js`, with nothing of `ws` present.
const FastBuffer: any = BufferCtor[Symbol.species];
const source = BufferCtor.from([1, 2, 3, 4, 5, 6]);
const sliced = new FastBuffer(source.buffer, source.byteOffset + 2, 3);
const slicedLen = sliced.length;
const slicedFirst = sliced[0];
const slicedLast = sliced[2];
const slicedIsU8 = sliced instanceof Uint8Array;
// It is a VIEW and not a copy, which is the only reason `ws` uses the species
// rather than `Buffer.from`: a copy per received frame is what the constant
// exists to avoid.
source[2] = 99;
const slicedAliases = sliced[0] === 99;

// The getter answers its RECEIVER, which is the whole point of the hook: a
// subclass inherits it and therefore names itself.
class MyBytes extends Uint8Array {}
const subclassSpecies = speciesOf(MyBytes) === MyBytes;

// Read the descriptor from whichever holder carries it, because Node's is
// `%TypedArray%` and this engine's is the class itself — see the header.
function speciesDescriptor(from: any): any {
  let holder = from;
  while (holder !== null && holder !== undefined) {
    const found = Object.getOwnPropertyDescriptor(holder, Symbol.species);
    if (found) return found;
    holder = Object.getPrototypeOf(holder);
  }
  return undefined;
}

const td = speciesDescriptor(Uint8Array);
const tdGetter = typeof td?.get;
const tdValue = td?.value;
const tdEnumerable = td?.enumerable;
const tdConfigurable = td?.configurable;

const abd = speciesDescriptor(ArrayBuffer);
const abdGetter = typeof abd?.get;
const abdConfigurable = abd?.configurable;

describe("Symbol.species on the typed arrays and the buffers", () => {
  test("each of the eleven typed arrays answers ITSELF, never a sibling", () => {
    expect(int8).toBe(true);
    expect(uint8).toBe(true);
    expect(uint8c).toBe(true);
    expect(int16).toBe(true);
    expect(uint16).toBe(true);
    expect(int32).toBe(true);
    expect(uint32).toBe(true);
    expect(f32).toBe(true);
    expect(f64).toBe(true);
    expect(bi64).toBe(true);
    expect(bu64).toBe(true);
  });

  test("ArrayBuffer and SharedArrayBuffer each answer themselves", () => {
    expect(ab).toBe(true);
    expect(sab).toBe(true);
  });

  test("DataView has no species, as the specification and Node both say", () => {
    expect(dv).toBe(undefined);
  });

  test("Buffer reaches a species through Uint8Array rather than its own", () => {
    expect(bufferParent).toBe(true);
    expect(bufferSpeciesKind).toBe("function");
  });

  test("the read ws performs builds a VIEW over the received bytes", () => {
    expect(slicedLen).toBe(3);
    expect(slicedFirst).toBe(3);
    expect(slicedLast).toBe(5);
    expect(slicedIsU8).toBe(true);
    expect(slicedAliases).toBe(true);
  });

  test("a subclass inherits the getter and so names itself", () => {
    expect(subclassSpecies).toBe(true);
  });

  test("the hook is an accessor, not enumerable, and configurable", () => {
    expect(tdGetter).toBe("function");
    expect(tdValue).toBe(undefined);
    expect(tdEnumerable).toBe(false);
    expect(tdConfigurable).toBe(true);
    expect(abdGetter).toBe("function");
    expect(abdConfigurable).toBe(true);
  });
});
