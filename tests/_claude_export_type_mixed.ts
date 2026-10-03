// `export { type A, B }` — the per-specifier modifier.
//
// The marked name goes and the unmarked one stays, so an export list is not
// all-or-nothing. The bare `export {}` at the end is the case erasure must NOT
// produce: it was written empty, and TypeScript keeps it.
type Hidden = { z: number };
const inner = 9;

export const keep = 7;

export { type Hidden, keep as kept };
export { inner as innerOut };
export {};
