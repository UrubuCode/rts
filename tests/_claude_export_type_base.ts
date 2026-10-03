// Base module for `claude-export-type-elision.test.ts`.
//
// It has both halves on purpose: a type that only exists at compile time and a
// value that exists at run time. A module of types alone would not tell the
// test anything, because the interesting case is a file that imports BOTH and
// re-exports only the type.
export interface Cfg {
  a: number;
}
export const marker = "base";
