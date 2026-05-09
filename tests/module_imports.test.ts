// Cobre issue #213: import/export entre modulos do usuario.
// Cada `import` aqui exercita uma forma diferente de `export`:
// - `export function` / `export const`
// - `export default function`
// - `export { x } from "./mod"` (re-export)
// - `export * from "./mod"` (re-export estrela)
//
// Suite roda via JIT (`rts test`); o caminho AOT compartilha o mesmo
// flatten do module graph desde o fix da issue #213, entao se isto
// passa, AOT multi-modulo tambem.

import { describe, test, expect } from "rts:test";
import { add, greet, MAGIC } from "./_module_lib";
import { double } from "./_module_reexport";
import answer from "./_module_default";
import { one, two, three } from "./_module_star_reexport";

const sum = add(3, 4);
const hi = greet("world");
const m = MAGIC;
const dbl = double(21);
const def = answer();
const o = one();
const t = two();
const th = three;

describe("module_imports", () => {
    test("named function", () => expect(sum).toBe(7));
    test("named function (string return)", () =>
        expect(hi).toBe("hello, world"));
    test("named const", () => expect(m).toBe(42));
    test("local + re-export coexist", () => expect(dbl).toBe(42));
    test("default export", () => expect(def).toBe(42));
    test("export *  one()", () => expect(o).toBe(1));
    test("export *  two()", () => expect(t).toBe(2));
    test("export *  const", () => expect(th).toBe(3));
});
