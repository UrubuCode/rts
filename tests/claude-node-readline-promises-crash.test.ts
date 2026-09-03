// node:readline/promises — createInterface() CRASHES THE PROCESS, every
// single time it is called, unconditionally. This is the module's entire
// point (Interface.question() as a Promise needs an Interface first), so
// this bug makes the whole module unreachable — nothing beyond
// claude-node-readline-promises.test.ts's one identity check survives it.
//
// ROOT CAUSE (found by isolating the call, then reading the source): merely
// importing "node:readline/promises" already forces "node:readline"'s own
// `namespace()` to run first — `lib.rs` derives the "node:readline/promises"
// module by reading the `.promises` member back OFF "node:readline"'s
// already-built namespace object (see readline/mod.rs's own doc for why:
// so `readline.promises === require('readline/promises')` holds). Building
// that namespace calls
//   entry::make_prototype(context, "Interface", INTERFACE_METHODS)
// from readline/mod.rs, which registers the prototype name "Interface"
// attributed to that call site.
//
// Then the FIRST TIME anything actually calls
// `readlinePromises.createInterface(...)`, `readline/promises.rs`'s own
// `create_interface` does:
//   let base = entry::make_prototype(context, "Interface", super::INTERFACE_METHODS);
// — the SAME name, "Interface", from a DIFFERENT source location
// (readline/promises.rs). `make_prototype`'s collision guard tracks
// registration by call site rather than by content, so even though both
// calls pass the identical `INTERFACE_METHODS` table (readline/promises.rs
// reuses `super::INTERFACE_METHODS` verbatim, per its own doc), it treats
// the second registration as two modules disagreeing about one prototype
// name and panics:
//
//   make_prototype("Interface") collision: already owned by
//   crates\rts-node\src\readline\mod.rs, also claimed by
//   crates\rts-node\src\readline\promises.rs
//
// CONFIRMED: reproduces with ONLY "node:readline/promises" imported (never
// "node:readline" directly — the transitive build above still happens), and
// reproduces identically via `readline.promises.createInterface(...)`
// reached the other way, off `require("node:readline")`. There is no
// argument shape or import order that avoids it: the "Interface" prototype
// is claimed by mod.rs's `namespace()` before user code runs at all,
// unconditionally, on any program that touches either specifier.
//
// Real Node, for comparison (`node -e "require('readline/promises')
// .createInterface({input, output})"`): builds an Interface with a working
// `.question()`, no different from the callback form otherwise.
import { describe, test, expect } from "rts:test";
import { createInterface } from "node:readline/promises";
import { Readable, Writable } from "node:stream";

const output: any = new Writable({ write: (_c: any, _e: any, cb: any) => cb() });
const input: any = new Readable({ read: () => {} });
const rl: any = createInterface({ input, output }); // <-- the killer call: crashes the process

describe("node:readline/promises createInterface (CRASHES on RTS)", () => {
    test("builds an Interface with a working question() (Node's answer)", async () => {
        expect(typeof rl.question).toBe("function");
    });
});
