// The `WebAssembly` global: a module compiles, instantiates, and its exported
// functions are callable from JavaScript.
//
// Every expected value here was MEASURED under node 22.23.2 before a line of the
// implementation was written — `validate` of garbage, the i32 wrap of
// `add(2147483647, 1)`, the `ToNumber` of `add("4", 5)`, the shape of
// `Module.exports`, and which constructor a bad module's error has. The module
// below is hand-assembled rather than produced by a toolchain so the fixture has
// no build step and the bytes can be read.
//
// What is deliberately NOT here: `Memory.buffer`. A linear memory reachable from
// JavaScript has to be an `ArrayBuffer` whose bytes ARE the wasm memory, and this
// engine's buffers own their allocation — the same wall `napi_create_external_buffer`
// refuses at, in writing. A `buffer` that copied would let a program write 42 and
// read 42 back while the wasm side never saw it, which is the hollow surface the
// `sync` namespace was deleted for. It arrives with the external buffer, measured
// by a fixture of its own.
import { test, expect } from "rts:test";

// (module
//   (func (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)
//   (func (param i32 i32) (result i32) local.get 0 local.get 1 i32.mul)
//   (memory 1)
//   (export "add" (func 0)) (export "mul" (func 1)) (export "memory" (memory 0)))
const bytes = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f,
  0x03, 0x03, 0x02, 0x00, 0x00,
  0x05, 0x03, 0x01, 0x00, 0x01,
  0x07, 0x16, 0x03,
  0x03, 0x61, 0x64, 0x64, 0x00, 0x00,
  0x03, 0x6d, 0x75, 0x6c, 0x00, 0x01,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x0a, 0x11, 0x02,
  0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
  0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6c, 0x0b,
]);

test("the global is an object", () => {
  expect(typeof WebAssembly).toBe("object");
});

test("validate answers true for a module", () => {
  expect(WebAssembly.validate(bytes)).toBe(true);
});

test("and false for bytes that are not one", () => {
  expect(WebAssembly.validate(new Uint8Array([1, 2, 3, 4]))).toBe(false);
});

test("new Module answers a Module", () => {
  const module = new WebAssembly.Module(bytes);
  expect(module instanceof WebAssembly.Module).toBe(true);
});

test("Module.exports names every export and its kind", () => {
  const described = WebAssembly.Module.exports(new WebAssembly.Module(bytes));
  expect(described.length).toBe(3);
  expect(described[0].name).toBe("add");
  expect(described[0].kind).toBe("function");
  expect(described[2].name).toBe("memory");
  expect(described[2].kind).toBe("memory");
});

test("Module.imports is empty for a module with none", () => {
  expect(WebAssembly.Module.imports(new WebAssembly.Module(bytes)).length).toBe(0);
});

test("an instance's export is a function", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes));
  expect(instance instanceof WebAssembly.Instance).toBe(true);
  expect(typeof instance.exports.add).toBe("function");
});

test("and calling it runs the wasm body", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes));
  expect(instance.exports.add(2, 3)).toBe(5);
  expect(instance.exports.mul(6, 7)).toBe(42);
});

// The two exports share one trampoline, so this is what proves the environment
// carries each one's identity rather than the last one installed winning.
test("two exports of one instance stay apart", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes));
  expect(instance.exports.add(10, 10)).toBe(20);
  expect(instance.exports.mul(10, 10)).toBe(100);
  expect(instance.exports.add(1, 1)).toBe(2);
});

// Two instances of one module have separate state; the same trampoline serves
// both, so this is the other half of the same question.
test("two instances of one module stay apart", () => {
  const module = new WebAssembly.Module(bytes);
  const first = new WebAssembly.Instance(module);
  const second = new WebAssembly.Instance(module);
  expect(first.exports.add(1, 2)).toBe(3);
  expect(second.exports.mul(3, 4)).toBe(12);
});

test("an i32 result wraps the way wasm says", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes));
  expect(instance.exports.add(2147483647, 1)).toBe(-2147483648);
  expect(instance.exports.add(-1, 1)).toBe(0);
});

// `ToNumber` on the way in, which is the JS-API's own conversion and not wasmi's.
test("an argument is coerced, not rejected", () => {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes));
  expect(instance.exports.add("4", 5)).toBe(9);
});

test("compile answers a promise of a Module", async () => {
  const module = await WebAssembly.compile(bytes);
  expect(module instanceof WebAssembly.Module).toBe(true);
});

test("instantiate answers both halves", async () => {
  const pair = await WebAssembly.instantiate(bytes);
  expect(pair.module instanceof WebAssembly.Module).toBe(true);
  expect(pair.instance instanceof WebAssembly.Instance).toBe(true);
  expect(pair.instance.exports.add(10, 5)).toBe(15);
});

test("a module that does not compile raises a CompileError", () => {
  let caught: any = null;
  try {
    new WebAssembly.Module(new Uint8Array([0, 0, 0, 0]));
  } catch (error) {
    caught = error;
  }
  expect(caught === null).toBe(false);
  expect(caught instanceof WebAssembly.CompileError).toBe(true);
  expect(caught instanceof Error).toBe(true);
  expect(caught.name).toBe("CompileError");
});

test("compile rejects rather than throwing", async () => {
  let rejected = false;
  try {
    await WebAssembly.compile(new Uint8Array([0, 0, 0, 0]));
  } catch (error) {
    rejected = error instanceof WebAssembly.CompileError;
  }
  expect(rejected).toBe(true);
});

test("the three error classes are Errors", () => {
  expect(WebAssembly.CompileError.prototype instanceof Error).toBe(true);
  expect(WebAssembly.LinkError.prototype instanceof Error).toBe(true);
  expect(WebAssembly.RuntimeError.prototype instanceof Error).toBe(true);
});

// `typeof WebAssembly !== "undefined"` is the whole of what most packages ask,
// and a package that asks it then reads one of these three names.
test("the names a package feature-detects are all present", () => {
  expect(typeof WebAssembly.compile).toBe("function");
  expect(typeof WebAssembly.instantiate).toBe("function");
  expect(typeof WebAssembly.validate).toBe("function");
  expect(typeof WebAssembly.Module).toBe("function");
  expect(typeof WebAssembly.Instance).toBe("function");
});
