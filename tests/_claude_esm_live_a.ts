// Helper for tests/claude-esm-live-binding.test.ts. The `_` prefix keeps the
// suite from collecting it as a test of its own.
//
// `n` is exported and then REASSIGNED by a function this module also exports.
// That is the whole of what a live binding is: the importer must see the
// assignment, because a named import is an indirect binding to this module's
// own slot and not a copy of what the slot held when the import ran.

export let n = 1;

export function bump() {
    n = 2;
}

export const K = 5;
