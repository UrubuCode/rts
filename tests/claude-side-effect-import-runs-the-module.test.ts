// `import "m"` runs the module — exactly once, and in source order.
//
// Register-by-import is how a polyfill, a `reflect-metadata`, a format registry
// or an `import "./setup"` in a test file does its work: the module exports
// nothing and its top-level statements ARE the point. None of it ran here.
// Every module of the program was compiled into the one compilation, so the
// body was emitted — it was simply never called, because since #2852 a module
// body runs when something NAMES it at run time and a side-effect import named
// nothing. `require("./m")` of the same file in the same graph did run it,
// which is what said the module was reachable and the ESM door was the fault.
//
// The visible cost was three layers from the cause: the `kfg` library registers
// its TypeBox string formats from `import "./utils/formats"`, so the registry
// held zero formats where bun holds eight and a valid email was rejected by a
// `c.Email` schema.
//
// Expected values measured in bun 1.4 on 2026-10-03 (`node` cannot run these
// files: it demands an extension on an ESM relative import).
import { describe, test, expect } from "rts:test";

// The four spellings and shapes that must all evaluate the module, written in
// the order whose effects this file then asserts:
//   - a side-effect import of a module that makes one of its own (`a` → `adep`)
//   - a second, so order between two of them is observable
//   - the side-effect module itself, which exports NOTHING
//   - a value import of a module that side-effect-imports it (transitive)
//   - `import {} from "m"`, the written-empty list
import "./_claude_side_effect_a";
import "./_claude_side_effect_b";
import "./_claude_side_effect_runs";
import { mark } from "./_claude_side_effect_mid";
import { two } from "./_claude_side_effect_second";
import { order, runs, store } from "./_claude_side_effect_registry";

describe("a side-effect import", () => {
  test("runs the module it names", () => {
    expect(store.length).toBe(2);
    expect(store[0]).toBe("alfa");
    expect(store[1]).toBe("beta");
  });

  test("runs a module that exports nothing at all", () => {
    // `_claude_side_effect_runs` has no `export`, so it has no namespace —
    // the case a namespace read would have thrown on.
    expect(runs.n).toBe(1);
  });

  test("runs it ONCE however many modules import it for its effects", () => {
    // Three importers: this file, `_mid` and `_second`. One evaluation —
    // the ESM rule, and the thing an always-evaluate fix would break
    // silently.
    expect(runs.n).toBe(1);
    expect(store.length).toBe(2);
  });

  test("evaluates in source order, dependencies before the module itself", () => {
    expect(order.join(",")).toBe("adep,a,b,side,mid,second");
  });

  test("carries the modules reached only transitively", () => {
    expect(mark).toBe(1);
    expect(two).toBe(2);
  });

  test("agrees with `require` of the same module, which already worked", () => {
    // The control that located the defect: the CommonJS door ran this module
    // the whole time. Asked again of a module that has already run, it
    // answers without running it a second time — so the two doors share one
    // evaluation rather than each having its own.
    require("./_claude_side_effect_runs");
    expect(runs.n).toBe(1);
    expect(store.length).toBe(2);
  });
});
