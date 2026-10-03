// `export type` is erased, exactly as `import type` is.
//
// What this pins is what TypeScript MEANS: a type-only export names nothing
// that exists at run time, so the declaration is deleted — not exported, not
// checked, not emitted. Every expectation below was measured in bun 1.4.0 on
// 2026-10-03 and the engine is asserted against those answers.
//
// Before the fix `parse::module::export_decl` read neither `type_only` on the
// declaration nor `is_type_only` on a specifier, while `import_decl` two
// functions above read both. The same rule, applied on one side of the module
// boundary and never on the other. The visible failures were two:
//
//   - `export type { Cfg }` without a source reached the checker as an
//     ordinary export, and `check::module::unresolvable_export` refused the
//     file: ``Syntax("`Cfg` is exported and never declared")``.
//   - `export type { Cfg } from "m"` compiled, because that check only looks
//     at the sourceless form — and was emitted as a run-time re-export. A
//     namespace import saw a key bun does not have.
import { describe, test, expect } from "rts:test";

import { use as useInline } from "./_claude_export_type_inline";
import { use as useSeparate } from "./_claude_export_type_separate";
import * as fromNs from "./_claude_export_type_from";
import * as mixedNs from "./_claude_export_type_mixed";

describe("export type is erased", () => {
  test("a file re-exporting an inline-imported type still compiles and runs", () => {
    // `import { marker, type Cfg }` + `export type { Cfg }`.
    expect(useInline({ a: 1 })).toBe("base:1");
  });

  test("a file re-exporting a separately imported type compiles and runs", () => {
    // `import type { Cfg }` + `export type { Cfg }`.
    expect(useSeparate({ a: 2 })).toBe(2);
  });

  test("`export type { X } from \"m\"` puts no key in the namespace", () => {
    // bun: `Object.keys(ns)` is `marker,use` — `Cfg` is absent. It used to be
    // present here, which is the wrongness that looks like it works: the key
    // was there and reading it answered `undefined`.
    expect("Cfg" in fromNs).toBe(false);
    expect(Object.keys(fromNs).sort().join(",")).toBe("marker,use");
  });

  test("`export *` beside it keeps forwarding the value", () => {
    // There is no `export type *` in the language, so nothing about the star
    // form is type-only and it must not be touched.
    expect((fromNs as { marker: string }).marker).toBe("base");
    expect(fromNs.use({ a: 3 })).toBe(3);
  });

  test("`export { type A, B }` drops only the marked specifier", () => {
    const keys = Object.keys(mixedNs).sort().join(",");
    expect(keys).toBe("innerOut,keep,kept");
    expect("Hidden" in mixedNs).toBe(false);
    expect(mixedNs.keep).toBe(7);
    expect((mixedNs as { kept: number }).kept).toBe(7);
    expect((mixedNs as { innerOut: number }).innerOut).toBe(9);
  });
});
