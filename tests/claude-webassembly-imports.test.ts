// The import object: a JavaScript function called from inside a wasm body.
//
// Every expected value measured under node 22.23.2 before the implementation
// existed. The decisive case is `a callback reads and writes the memory mid-call`:
// it is what the previous lot's mirroring — one copy in before the call, one out
// after — answers WRONGLY, and it is not a corner. The `wasm-bindgen` bridge
// `@whiskeysockets/baileys` carries imports 99 functions and 35 of them read the
// linear memory from inside the call.
import { test, expect } from "rts:test";

// (module
//   (import "env" "twice" (func $twice (param i32) (result i32)))
//   (import "env" "note" (func $note (param i32)))
//   (memory 1)
//   (func (param i32) (result i32) local.get 0 call $twice)
//   (func (param i32) local.get 0 call $note)
//   (export "useTwice" (func 2)) (export "useNote" (func 3)) (export "memory" (memory 0)))
const calling = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x0a, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f, 0x60, 0x01, 0x7f, 0x00,
  0x02, 0x18, 0x02,
  0x03, 0x65, 0x6e, 0x76, 0x05, 0x74, 0x77, 0x69, 0x63, 0x65, 0x00, 0x00,
  0x03, 0x65, 0x6e, 0x76, 0x04, 0x6e, 0x6f, 0x74, 0x65, 0x00, 0x01,
  0x03, 0x03, 0x02, 0x00, 0x01,
  0x05, 0x03, 0x01, 0x00, 0x01,
  0x07, 0x1f, 0x03,
  0x08, 0x75, 0x73, 0x65, 0x54, 0x77, 0x69, 0x63, 0x65, 0x00, 0x02,
  0x07, 0x75, 0x73, 0x65, 0x4e, 0x6f, 0x74, 0x65, 0x00, 0x03,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x0a, 0x0f, 0x02,
  0x06, 0x00, 0x20, 0x00, 0x10, 0x00, 0x0b,
  0x06, 0x00, 0x20, 0x00, 0x10, 0x01, 0x0b,
]);

// (module
//   (import "env" "peekAt" (func $peekAt (param i32) (result i32)))
//   (memory 1)
//   (func (param i32) (result i32) i32.const 0 i32.const 7 i32.store8 i32.const 0 call $peekAt)
//   (export "writeThenAsk" (func 1)) (export "memory" (memory 0)))
const crossing = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
  0x02, 0x0e, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x06, 0x70, 0x65, 0x65, 0x6b, 0x41, 0x74, 0x00, 0x00,
  0x03, 0x02, 0x01, 0x00,
  0x05, 0x03, 0x01, 0x00, 0x01,
  0x07, 0x19, 0x02,
  0x0c, 0x77, 0x72, 0x69, 0x74, 0x65, 0x54, 0x68, 0x65, 0x6e, 0x41, 0x73, 0x6b, 0x00, 0x01,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x0a, 0x0f, 0x01,
  0x0d, 0x00, 0x41, 0x00, 0x41, 0x07, 0x3a, 0x00, 0x00, 0x41, 0x00, 0x10, 0x00, 0x0b,
]);

test("Module.imports names what a module asks for", () => {
  const asked: any = WebAssembly.Module.imports(new WebAssembly.Module(calling));
  expect(asked.length).toBe(2);
  expect(asked[0].module).toBe("env");
  expect(asked[0].name).toBe("twice");
  expect(asked[0].kind).toBe("function");
  expect(asked[1].name).toBe("note");
});

test("an imported function is called, and its answer comes back", () => {
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(calling), {
    env: { twice: (n: number) => n * 2, note: () => {} },
  });
  expect(made.exports.useTwice(21)).toBe(42);
});

test("and its arguments arrive", () => {
  const seen: number[] = [];
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(calling), {
    env: { twice: (n: number) => n, note: (n: number) => { seen.push(n); } },
  });
  made.exports.useNote(5);
  expect(seen.length).toBe(1);
  expect(seen[0]).toBe(5);
});

test("two imports stay apart", () => {
  const seen: number[] = [];
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(calling), {
    env: { twice: (n: number) => n + 1000, note: (n: number) => { seen.push(n); } },
  });
  expect(made.exports.useTwice(1)).toBe(1001);
  made.exports.useNote(2);
  expect(seen[0]).toBe(2);
});

// THE decisive one. The callback runs mid-call: it reads the byte the wasm body
// just wrote, and writes one the wasm body has not read yet. Mirroring only
// around the call answers `0` for the first and loses the second.
test("a callback reads and writes the memory mid-call", () => {
  let view: any = null;
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(crossing), {
    env: {
      peekAt: (at: number) => {
        const seen = view[at];
        view[at + 1] = 99;
        return seen;
      },
    },
  });
  view = new Uint8Array(made.exports.memory.buffer);
  expect(made.exports.writeThenAsk(0)).toBe(7);
  expect(view[1]).toBe(99);
});

test("a missing import namespace is a TypeError", () => {
  let caught: any = null;
  try {
    new WebAssembly.Instance(new WebAssembly.Module(calling), {});
  } catch (error) {
    caught = error;
  }
  expect(caught === null).toBe(false);
  expect(caught instanceof TypeError).toBe(true);
});

// The other half of the same division, and the reason it is measured rather than
// guessed: a namespace that EXISTS with a member missing is a `LinkError`, not a
// `TypeError`, and a program branches on which.
test("a missing member of a namespace that exists is a LinkError", () => {
  let caught: any = null;
  try {
    new WebAssembly.Instance(new WebAssembly.Module(calling), { env: { twice: (n: number) => n } });
  } catch (error) {
    caught = error;
  }
  expect(caught === null).toBe(false);
  expect(caught instanceof WebAssembly.LinkError).toBe(true);
});

test("an import that is not a function is refused", () => {
  let threw = false;
  try {
    new WebAssembly.Instance(new WebAssembly.Module(calling), { env: { twice: 7, note: 8 } });
  } catch {
    threw = true;
  }
  expect(threw).toBe(true);
});

// A throw inside an import has to come out as the program's OWN error, not as a
// `RuntimeError` wearing its place — which is what consuming the pending throw to
// build a trap message would produce.
test("a throw inside a callback propagates as itself", () => {
  const made: any = new WebAssembly.Instance(new WebAssembly.Module(calling), {
    env: {
      twice: () => {
        throw new RangeError("from the callback");
      },
      note: () => {},
    },
  });
  let caught: any = null;
  try {
    made.exports.useTwice(1);
  } catch (error) {
    caught = error;
  }
  expect(caught === null).toBe(false);
  expect(caught instanceof RangeError).toBe(true);
  expect(String(caught.message)).toBe("from the callback");
});

// (module
//   (import "env" "cb" (func $cb (param i32) (result i32)))
//   (memory 1)
//   (func (param i32) (result i32) local.get 0 call $cb)   ;; "outer", func 1
//   (func (param i32) (result i32)                         ;; "bump",  func 2
//     i32.const 1 memory.grow drop                         ;; one page more
//     i32.const 65552 local.get 0 i32.store8               ;; into the new page
//     memory.size)
//   (export "outer" (func 1)) (export "bump" (func 2)) (export "memory" (memory 0)))
//
// Sizes: types 6, imports 10, funcs 3, memory 3, exports 25 (3 + 8 + 7 + 9),
// code 27 (1 + 7 + 19). `bump`'s body is 18 bytes after its locals count.
const nested = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
  0x02, 0x0a, 0x01, 0x03, 0x65, 0x6e, 0x76, 0x02, 0x63, 0x62, 0x00, 0x00,
  0x03, 0x03, 0x02, 0x00, 0x00,
  0x05, 0x03, 0x01, 0x00, 0x01,
  0x07, 0x19, 0x03,
  0x05, 0x6f, 0x75, 0x74, 0x65, 0x72, 0x00, 0x01,
  0x04, 0x62, 0x75, 0x6d, 0x70, 0x00, 0x02,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x0a, 0x1b, 0x02,
  0x06, 0x00, 0x20, 0x00, 0x10, 0x00, 0x0b,
  0x12, 0x00, 0x41, 0x01, 0x40, 0x00, 0x1a, 0x41, 0x90, 0x80, 0x04, 0x20, 0x00,
  0x3a, 0x00, 0x00, 0x3f, 0x00, 0x0b,
]);

// This used to fix a REFUSAL, with node's answers beside it as the divergence.
// The argument for refusing was that `wasm-bindgen` needs reentrancy only for its
// finalizers, which run from a `FinalizationRegistry` and therefore after the
// call. That premise is true and the conclusion was wrong: the caller that needs
// it is `__wbindgen_malloc`, an export the import itself calls to hand data back,
// and the `whatsapp-rust-bridge` that `@whiskeysockets/baileys` uses for `hkdf`
// raised on its FIRST call. `reentry.rs` is the mechanism and its invariant.
test("a callback calls an export of the SAME instance", () => {
  let made: any = null;
  const seen: number[] = [];
  made = new WebAssembly.Instance(new WebAssembly.Module(calling), {
    env: {
      twice: (n: number) => {
        made.exports.useNote(8);
        return n * 2;
      },
      note: (n: number) => {
        seen.push(n);
      },
    },
  });
  expect(made.exports.useTwice(3)).toBe(6);
  expect(seen.length).toBe(1);
  expect(seen[0]).toBe(8);
});

// The decisive one for the MEMORY, and measured under node 22.23.2 before any of
// this existed: `outer(21) = 42`, the reentrant `bump` answers 2, the buffer is
// 131072 bytes afterwards and byte 65552 is 42.
//
// It is decisive because the reentrant export GROWS the linear memory, which is
// what `__wbindgen_malloc` does on its first allocation, and `memory.rs` mirrors
// at every traversal of control — so a reentrant call is one traversal more in
// each direction and the mirror has to follow the growth. Writing past the end of
// a `Uint8Array` is silently ignored, so getting this wrong answers a plausible
// number rather than an error: it is how `md5("hello")` once came back as the
// hash of zeros.
test("a reentrant export may grow the memory, and the buffer follows", () => {
  let made: any = null;
  let fromInside = 0;
  made = new WebAssembly.Instance(new WebAssembly.Module(nested), {
    env: {
      cb: (n: number) => {
        fromInside = made.exports.bump(42);
        return n * 2;
      },
    },
  });
  expect(made.exports.outer(21)).toBe(42);
  expect(fromInside).toBe(2);
  expect(made.exports.memory.buffer.byteLength).toBe(131072);
  const view: any = new Uint8Array(made.exports.memory.buffer);
  expect(view[65552]).toBe(42);
});

// Two instances suspended at once, and the innermost frame is NOT the one the
// call needs: A's import calls B, and B's import calls back into A. node answers
// 30 and sees 4. It is the case that decides `reentry::use_active` has to search
// the stack by instance rather than take its top.
test("a callback reaches an outer instance past an inner one", () => {
  const module = new WebAssembly.Module(calling);
  const seen: number[] = [];
  let outer: any = null;
  const inner: any = new WebAssembly.Instance(module, {
    env: {
      twice: (n: number) => {
        outer.exports.useNote(n + 1);
        return n * 10;
      },
      note: () => {},
    },
  });
  outer = new WebAssembly.Instance(module, {
    env: {
      twice: (n: number) => inner.exports.useTwice(n),
      note: (n: number) => {
        seen.push(n);
      },
    },
  });
  expect(outer.exports.useTwice(3)).toBe(30);
  expect(seen.length).toBe(1);
  expect(seen[0]).toBe(4);
});

test("a callback into ANOTHER instance works", () => {
  const module = new WebAssembly.Module(calling);
  const other: any = new WebAssembly.Instance(module, {
    env: { twice: (n: number) => n + 100, note: () => {} },
  });
  const made: any = new WebAssembly.Instance(module, {
    env: { twice: (n: number) => other.exports.useTwice(n), note: () => {} },
  });
  expect(made.exports.useTwice(1)).toBe(101);
});
