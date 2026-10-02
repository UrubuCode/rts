import { describe, test, expect } from "rts:test";

// The abstract typed-array prototype — `%TypedArray%.prototype` in the
// specification — exists, sits between each concrete prototype and
// `Object.prototype`, and carries `Symbol.toStringTag` as an ACCESSOR.
//
// Why this is worth a fixture of its own: the tag already read correctly
// (`new Int8Array()[Symbol.toStringTag]` answered "Int8Array") because each
// concrete prototype carries it as a DATA property. What did not work is the
// question a real library asks about it rather than through it —
// `safe-stable-stringify`, which `pino` and therefore `@whiskeysockets/baileys`
// depend on, reads
//
//   Object.getOwnPropertyDescriptor(
//     Object.getPrototypeOf(Object.getPrototypeOf(new Int8Array())),
//     Symbol.toStringTag,
//   ).get
//
// and calls that getter on arbitrary values to classify them. With the level
// missing, the inner `getPrototypeOf` answered `Object.prototype`, the
// descriptor was `undefined`, and reading `.get` off it ended the program.
//
// Every value below was measured on Node 22.23.2, not derived from the text of
// the specification.

const abstractProto = Object.getPrototypeOf(Int8Array.prototype);
const viaInstance = Object.getPrototypeOf(Object.getPrototypeOf(new Int8Array()));

const descriptor = Object.getOwnPropertyDescriptor(abstractProto, Symbol.toStringTag);
const getter = descriptor === undefined ? undefined : descriptor.get;

describe("the abstract typed-array prototype", () => {
  test("every concrete prototype inherits from one shared object", () => {
    expect(viaInstance === abstractProto).toBe(true);
    expect(Object.getPrototypeOf(Uint8Array.prototype) === abstractProto).toBe(true);
    expect(Object.getPrototypeOf(Float64Array.prototype) === abstractProto).toBe(true);
    expect(Object.getPrototypeOf(BigInt64Array.prototype) === abstractProto).toBe(true);
  });

  test("it is a level of its own, not Object.prototype", () => {
    expect(abstractProto === Object.prototype).toBe(false);
    expect(Object.getPrototypeOf(abstractProto) === Object.prototype).toBe(true);
  });
});

describe("its Symbol.toStringTag is an accessor", () => {
  test("the descriptor is a getter with no setter", () => {
    expect(descriptor !== undefined).toBe(true);
    expect(typeof getter).toBe("function");
    expect(descriptor.set === undefined).toBe(true);
    expect(descriptor.enumerable).toBe(false);
    expect(descriptor.configurable).toBe(true);
  });

  test("the getter describes itself the way the specification names it", () => {
    expect(getter.name).toBe("get [Symbol.toStringTag]");
    expect(getter.length).toBe(0);
  });

  test("called on a view it answers that view's class", () => {
    expect(getter.call(new Int8Array(1))).toBe("Int8Array");
    expect(getter.call(new Uint8Array(1))).toBe("Uint8Array");
    expect(getter.call(new Uint8ClampedArray(1))).toBe("Uint8ClampedArray");
    expect(getter.call(new Float64Array(1))).toBe("Float64Array");
    expect(getter.call(new BigInt64Array(1))).toBe("BigInt64Array");
  });

  // The whole reason the library reads the getter instead of the string: it is
  // a TEST, so it has to answer rather than throw for everything that is not a
  // typed array. A `DataView` is the sharp case — it is a view over a buffer
  // and still not one of the eight.
  test("called on anything else it answers undefined rather than throwing", () => {
    expect(getter.call({})).toBe(undefined);
    expect(getter.call(7)).toBe(undefined);
    expect(getter.call("x")).toBe(undefined);
    expect(getter.call(new DataView(new ArrayBuffer(8)))).toBe(undefined);
    expect(getter.call([1, 2])).toBe(undefined);
  });
});

describe("what the new level did not change", () => {
  test("the tag still reads directly off an instance", () => {
    expect(new Int8Array(1)[Symbol.toStringTag]).toBe("Int8Array");
    expect(Object.prototype.toString.call(new Int8Array(1))).toBe("[object Int8Array]");
  });

  test("the methods are still found through the chain", () => {
    const doubled = new Uint8Array([1, 2, 3]).map((v) => v * 2);
    expect(doubled.length).toBe(3);
    expect(doubled[2]).toBe(6);
  });
});
