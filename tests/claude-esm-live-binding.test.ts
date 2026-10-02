// A named `import` is a LIVE binding, not a snapshot.
//
// Measured on Node 22.23.2, 2026-10-02, from the same six modules written as
// `.mjs` — the numbers below are that run, not a reading of the specification:
//
//     simple 2        closure 2       ns 2        cycle 32:64
//     default 42 1    reexport 2      cjs 7 8     const 5
//     shadow 99       typeof function number
//
// What the engine answered before this file existed was `simple 1`, `closure 1`
// and `cycle undefined:undefined`: `emit/module.rs` lowered each binding to one
// `ModuleBinding` call stored in a local, so the import was whatever the slot
// held at the line the import was written. The cycle is the case that made it
// matter — it is what stopped `@whiskeysockets/baileys` building a socket.
//
// Every test here names the language behaviour it pins and never that a
// property read happened: the mechanism is `emit/module.rs`'s to change.

import { describe, test, expect } from "rts:test";

import { n, bump, K } from "./_claude_esm_live_a";
import { creds, probeNamespace } from "./_claude_esm_cycle_defaults";
import answer from "./_module_default";
import * as ns from "./_claude_esm_live_a";
import { n as reexported } from "./_claude_esm_live_reexport";
import { cv, cf } from "./_claude_esm_live_cjs";

// Read BEFORE the assignment, so the one test that pins a snapshot has
// something to compare against.
const beforeBump = n;

// Captured in a closure made before the assignment, and called after it: the
// closure must not have copied the value either.
const later = () => n;

bump();

// A parameter of the same spelling is a different binding, and the import must
// not reach into it.
function shadow(n: i64): i64 {
    return n;
}

describe("a named import is a live binding", () => {
    test("a read after the exporter reassigned sees the new value", () => {
        expect(n).toBe(2);
    });

    test("and the read before it saw the old one, so nothing was hoisted", () => {
        expect(beforeBump).toBe(1);
    });

    test("a closure made before the assignment reads it afterwards", () => {
        expect(later()).toBe(2);
    });

    test("the namespace of the same module agrees with the named import", () => {
        expect(ns.n).toBe(2);
    });

    test("a `const` export is unaffected", () => {
        expect(K).toBe(5);
    });
});

describe("a cycle reads the exporter's slot when the value is used", () => {
    // `_claude_esm_cycle_crypto` imports `KEY_BUNDLE_TYPE` at its top level,
    // which runs while `_claude_esm_cycle_defaults` has published nothing. A
    // snapshot there is `undefined`; Node answers `32:64`.
    test("a value published after the importer ran is still readable", () => {
        expect(creds()).toBe("32:64");
    });

    // THE TRAP: a namespace exists from the moment a module is ENTERED, not from
    // its first `export`. The re-entered half of the cycle reads `typeof` the
    // other's namespace on its own first line, before a single name has been
    // published — Node answers `object:64` and so must this, because every live
    // read in a cycle is a property of exactly that object.
    test("the namespace of a module that has published nothing is an object", () => {
        expect(probeNamespace()).toBe("object:64");
    });
});

describe("the other import forms keep working", () => {
    test("a default import is the module's default export", () => {
        expect(answer()).toBe(42);
    });

    test("an imported name used as a function is callable", () => {
        expect(typeof bump).toBe("function");
    });

    test("an import from a CommonJS module reads its `exports`", () => {
        expect(cv).toBe(7);
    });

    test("and a callable one is called", () => {
        expect(cf()).toBe(8);
    });

    test("`typeof` an imported name is the type of the value", () => {
        expect(typeof n).toBe("number");
    });

    test("a parameter of the same spelling shadows the import", () => {
        expect(shadow(99)).toBe(99);
    });
});

describe("what is NOT live, stated rather than discovered", () => {
    // `export { n } from "m"` is published as a VALUE at the end of the
    // re-exporting module's body, so an assignment made after that body ran is
    // not forwarded. Node answers 2 here; this engine answers 1.
    //
    // It is a different mechanism from the one this change fixes: a re-export
    // never binds the name locally, so there is no use site to read through —
    // making it live needs an accessor on the namespace, which is its own lot.
    test("a re-export is a snapshot taken when the re-exporting module ended", () => {
        expect(reexported).toBe(1);
    });
});
