// `export type { T } from "m"` — the form with a source.
//
// This one compiled before the fix and was still WRONG: the type was emitted as
// a run-time re-export, so a namespace import of this module carried a `Cfg`
// key. `export *` is here too, because it has no type-only spelling in the
// language and must keep forwarding the value.
export type { Cfg } from "./_claude_export_type_base";
export * from "./_claude_export_type_base";

export function use(c: { a: number }): number {
  return c.a;
}
