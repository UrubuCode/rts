// A wasm function of SIX parameters, in both directions.
//
// Every expected value below was measured under node 22.23.2 before the
// implementation existed (`sum6(1,2,4,8,16,32)` is 63, extra arguments are
// ignored, missing ones are 0, strings go through ToNumber, and the imported
// callback sees all six).
//
// Why six and not five: a native in this engine is handed FOUR argument slots,
// so a five-parameter module would prove only that the fifth arrived. The powers
// of two are what name the offender — a missing 5th reads 47, a missing 6th
// reads 31 — which is the difference between "an argument is lost" and "which
// one".
//
// This is the surface `wasm-bindgen` needs: its glue turns `hkdf(data, 64, info)`
// into `wasm.hkdf(retptr, ptr0, len0, expanded_length, addHeapObject(info))` —
// five numbers — so the whole bridge is unreachable while the fifth is dropped.
import { test, expect } from "rts:test";

// (module
//   (type (func (param i32 i32 i32 i32 i32 i32) (result i32)))
//   (import "env" "collect" (func $collect (type 0)))
//   (func (type 0) p0+p1+p2+p3+p4+p5)
//   (func (type 0) call $collect with all six)
//   (export "sum6" (func 1)) (export "relay6" (func 2)))
//
// Assembled byte by byte; the section sizes are the uleb128 length of each body
// that follows, and a wrong one is a `CompileError` rather than a wrong answer.
const sixes = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  // type section: one type, six i32 params, one i32 result
  0x01, 0x0b, 0x01, 0x60, 0x06, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x01, 0x7f,
  // import section: env.collect of that type
  0x02, 0x0f, 0x01, 0x03, 0x65, 0x6e, 0x76,
  0x07, 0x63, 0x6f, 0x6c, 0x6c, 0x65, 0x63, 0x74, 0x00, 0x00,
  // function section: two functions, both of type 0
  0x03, 0x03, 0x02, 0x00, 0x00,
  // export section: sum6 = func 1, relay6 = func 2
  0x07, 0x11, 0x02,
  0x04, 0x73, 0x75, 0x6d, 0x36, 0x00, 0x01,
  0x06, 0x72, 0x65, 0x6c, 0x61, 0x79, 0x36, 0x00, 0x02,
  // code section
  0x0a, 0x26, 0x02,
  0x13, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x20, 0x02, 0x6a, 0x20, 0x03,
  0x6a, 0x20, 0x04, 0x6a, 0x20, 0x05, 0x6a, 0x0b,
  0x10, 0x00, 0x20, 0x00, 0x20, 0x01, 0x20, 0x02, 0x20, 0x03, 0x20, 0x04,
  0x20, 0x05, 0x10, 0x00, 0x0b,
]);

function instance(collect: (...given: number[]) => number): any {
  return new WebAssembly.Instance(new WebAssembly.Module(sixes), { env: { collect } });
}

test("all six arguments of an exported wasm function arrive", () => {
  const made = instance(() => 0);
  // 1+2+4+8+16+32. A dropped fifth reads 47, a dropped sixth 31, a dropped
  // pair 15 — which is why the arguments are powers of two.
  expect(made.exports.sum6(1, 2, 4, 8, 16, 32)).toBe(63);
});

test("and in the order they were written", () => {
  const made = instance(() => 0);
  // Place value: a swap of any two positions changes the answer.
  expect(made.exports.sum6(100000, 10000, 1000, 100, 10, 1)).toBe(111111);
});

test("an argument past the parameters is ignored", () => {
  const made = instance(() => 0);
  expect(made.exports.sum6(1, 2, 4, 8, 16, 32, 64, 128)).toBe(63);
});

test("a missing argument is zero, not NaN", () => {
  const made = instance(() => 0);
  expect(made.exports.sum6(1, 2, 4)).toBe(7);
  expect(made.exports.sum6()).toBe(0);
});

test("a string argument past the fourth still goes through ToNumber", () => {
  const made = instance(() => 0);
  expect(made.exports.sum6("1", "2", "4", "8", "16", "32")).toBe(63);
});

test("an imported function is given all six of its parameters", () => {
  let seen: number[] = [];
  const made = instance((...given: number[]) => {
    seen = given;
    return given.length;
  });
  expect(made.exports.relay6(1, 2, 3, 4, 5, 6)).toBe(6);
  expect(seen.length).toBe(6);
  expect(seen.join(",")).toBe("1,2,3,4,5,6");
});

test("an imported function reading its fifth and sixth by name sees them", () => {
  // The rest parameter above and six declared parameters are two different
  // readers of the same call, and only one of them was ever broken here.
  let tail = 0;
  const made = instance((a: number, b: number, c: number, d: number, e: number, f: number) => {
    tail = e * 10 + f;
    return a + b + c + d;
  });
  expect(made.exports.relay6(1, 2, 3, 4, 7, 9)).toBe(10);
  expect(tail).toBe(79);
});

test("the arity an exported function reports counts every parameter", () => {
  const made = instance(() => 0);
  expect(made.exports.sum6.length).toBe(6);
});
