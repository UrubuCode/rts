// Re-export consumido por module_imports.test.ts.

export { add, greet } from "./_module_lib";

export function double(n: i64): i64 {
    return n * 2;
}
