// The `require` half of `claude-host-module-exports.test.ts`.
//
// It lives in a second file because the five CommonJS names are bound only for
// a module (`emit/function.rs`'s `if let Some(specifier) = module`), and a suite
// file that imports no FILE is compiled by the single-source path
// (`suite_run.rs`/`cli/new_engine.rs`), which has no specifier to bind them
// against. Importing this file puts the program on the graph path, where
// `require` exists — and it also makes the test read a required value the way a
// ported program does: from a module, not from an entry script.
export const requiredEvents = require("events");
export const requiredNodeEvents = require("node:events");
export const requiredFs = require("node:fs");
export const requiredWs = require("ws");
