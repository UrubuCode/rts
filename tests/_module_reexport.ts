// Re-export consumido por module_imports.test.ts.

export { add, greet } from "./_module_lib";

// Re-export com alias: o consumidor importa este modulo e ve `plus`,
// que internamente delega para `add` de `_module_lib`.
export { add as plus } from "./_module_lib";

export function double(n: i64): i64 {
    return n * 2;
}
