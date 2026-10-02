// `WebAssembly.Memory`: a linear memory JavaScript and the wasm side SHARE.
//
// This is the fixture the first lot deliberately did not have, and the reason is
// the only thing it really tests: `memory.buffer` must be an `ArrayBuffer` whose
// bytes ARE the wasm memory. A `buffer` that answered a copy would pass a naive
// test — write 42, read 42 back — while the wasm side never saw the write, which
// is why the assertions below cross the boundary in BOTH directions: JavaScript
// writes a byte and wasm reads it, then wasm writes one and JavaScript reads it.
//
// Measured under node 22.23.2: one page is 65536 bytes, `grow(1)` answers the
// previous page count and the buffer is then 131072. The old buffer is DETACHED
// by a grow, which the specification requires and `entry::detach_buffer` already
// implements for the language's own `transfer`.
import { test, expect } from "rts:test";

// (module
//   (memory 1)
//   (func (param i32) (result i32) local.get 0 i32.load8_u)   ;; peek
//   (func (param i32 i32) local.get 0 local.get 1 i32.store8) ;; poke
//   (export "memory" (memory 0)) (export "peek" (func 0)) (export "poke" (func 1)))
const bytes = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  // two types: (i32)->i32 and (i32,i32)->()
  0x01, 0x0b, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f, 0x60, 0x02, 0x7f, 0x7f, 0x00,
  0x03, 0x03, 0x02, 0x00, 0x01,
  0x05, 0x03, 0x01, 0x00, 0x01,
  0x07, 0x18, 0x03,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x04, 0x70, 0x65, 0x65, 0x6b, 0x00, 0x00,
  0x04, 0x70, 0x6f, 0x6b, 0x65, 0x00, 0x01,
  0x0a, 0x13, 0x02,
  0x07, 0x00, 0x20, 0x00, 0x2d, 0x00, 0x00, 0x0b,
  0x09, 0x00, 0x20, 0x00, 0x20, 0x01, 0x3a, 0x00, 0x00, 0x0b,
]);

function instance(): any {
  return new WebAssembly.Instance(new WebAssembly.Module(bytes));
}

test("a memory export is a Memory", () => {
  const memory = instance().exports.memory;
  expect(memory instanceof WebAssembly.Memory).toBe(true);
});

test("one page is 65536 bytes", () => {
  expect(instance().exports.memory.buffer.byteLength).toBe(65536);
});

test("the buffer is an ArrayBuffer", () => {
  expect(instance().exports.memory.buffer instanceof ArrayBuffer).toBe(true);
});

// THE decisive one in this direction: a copy would answer 42 here too.
test("what JavaScript writes, wasm reads", () => {
  const made = instance();
  new Uint8Array(made.exports.memory.buffer)[7] = 42;
  expect(made.exports.peek(7)).toBe(42);
});

// And the other direction, which a copy made at read time would also pass — the
// two together are what pin a shared buffer.
test("what wasm writes, JavaScript reads", () => {
  const made = instance();
  made.exports.poke(9, 99);
  expect(new Uint8Array(made.exports.memory.buffer)[9]).toBe(99);
});

test("two reads of buffer answer the same bytes", () => {
  const made = instance();
  new Uint8Array(made.exports.memory.buffer)[3] = 7;
  expect(new Uint8Array(made.exports.memory.buffer)[3]).toBe(7);
});

test("grow answers the page count it had", () => {
  const memory = instance().exports.memory;
  expect(memory.grow(1)).toBe(1);
  expect(memory.buffer.byteLength).toBe(131072);
});

test("and detaches the buffer it had", () => {
  const memory = instance().exports.memory;
  const before = memory.buffer;
  memory.grow(1);
  expect(before.byteLength).toBe(0);
  expect(before === memory.buffer).toBe(false);
});

test("new Memory({ initial }) answers one of its own", () => {
  const memory = new WebAssembly.Memory({ initial: 2 });
  expect(memory instanceof WebAssembly.Memory).toBe(true);
  expect(memory.buffer.byteLength).toBe(131072);
});

// A memory grows TWO ways and only one of them comes through `Memory.prototype.grow`:
// a wasm body runs the `memory.grow` INSTRUCTION itself, which is what
// `wasm-bindgen`'s allocator does on its first allocation. The buffer then had the
// old length, the bytes a program wrote landed past its end — and a write past the
// end of a `Uint8Array` is SILENTLY IGNORED.
//
// That is not hypothetical: `md5("hello")` through the `whatsapp-rust-bridge`
// answered `ca9c491a…`, the md5 of five zero bytes, where node answers
// `5d41402a…`. Nothing threw, nothing was empty, and the value looked like a hash.
// It was found by comparing against node — which is why this asserts a byte the
// wasm side READS BACK rather than that the lengths look right.
//
// (module (memory 1)
//   (func (result i32) i32.const 1 memory.grow)
//   (func (param i32) (result i32) local.get 0 i32.load8_u)
//   (export "growOne" (func 0)) (export "peek" (func 1)) (export "memory" (memory 0)))
const growing = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x0a, 0x02, 0x60, 0x00, 0x01, 0x7f, 0x60, 0x01, 0x7f, 0x01, 0x7f,
  0x03, 0x03, 0x02, 0x00, 0x01,
  0x05, 0x03, 0x01, 0x00, 0x01,
  0x07, 0x1b, 0x03,
  0x07, 0x67, 0x72, 0x6f, 0x77, 0x4f, 0x6e, 0x65, 0x00, 0x00,
  0x04, 0x70, 0x65, 0x65, 0x6b, 0x00, 0x01,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x0a, 0x10, 0x02,
  0x06, 0x00, 0x41, 0x01, 0x40, 0x00, 0x0b,
  0x07, 0x00, 0x20, 0x00, 0x2d, 0x00, 0x00, 0x0b,
]);

test("a grow the WASM side performs is followed by the buffer", () => {
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(growing));
  expect(made.exports.memory.buffer.byteLength).toBe(65536);
  expect(made.exports.growOne()).toBe(1);
  expect(made.exports.memory.buffer.byteLength).toBe(131072);
});

test("and a byte written past the old end reaches the wasm side", () => {
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(growing));
  made.exports.growOne();
  new Uint8Array(made.exports.memory.buffer)[70000] = 123;
  expect(made.exports.peek(70000)).toBe(123);
});
