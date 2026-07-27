// (#407) Top-level globals com handles sao registrados como GC roots
// via \`global_roots::add\` durante o JIT finalize. Combinado com
// transitive marking (#398), permite servidores de longa duracao
// com estado em memoria sem perder dados.

import { describe, test, expect } from "rts:test";

// 1. String global mutavel cresce em loop sem perder dados
let buffer = "";
for (let i = 0; i < 200; i++) {
    buffer = buffer + "x";
}
const buffer_len: i64 = new TextEncoder().encode(buffer).length;

// 2. String global em concatenacoes diversas
let log = "";
for (let i = 0; i < 50; i++) {
    log = log + "[" + i + "]";
}
const log_len: i64 = new TextEncoder().encode(log).length;

// 3. Map global preserva entries (com chaves estaticas — chaves
// dinamicas em loop e' issue separada de codegen).
const cache: any = { foo: "alpha", bar: "beta", baz: "gamma" };
for (let j = 0; j < 1000; j++) {
    const dummy = "g_" + j;
}
const cache0 = cache.foo;
const cache29 = cache.bar;

// 4. Array global preserva elementos
const items: string[] = [];
for (let i = 0; i < 50; i++) {
    items.push("item_" + i);
}
const items_len: i64 = items.length;
const items_first = items[0];
const items_last = items[49];

// 5. String global com concat em fn user (testa propagacao via call)
let counter_str = "";
function append(s: string): void {
    counter_str = counter_str + s + ",";
}
for (let i = 0; i < 100; i++) {
    append("step" + i);
}
const counter_len: i64 = new TextEncoder().encode(counter_str).length;

describe("gc_global_handles_as_roots", () => {
    test("global string buffer 200x", () => expect(buffer_len).toBe(200));
    test("global string log non-empty", () =>
        expect(log_len > 100).toBe(true));
    test("Map global foo", () => expect(cache0).toBe("alpha"));
    test("Map global bar", () => expect(cache29).toBe("beta"));
    test("Array global len", () => expect(items_len).toBe(50));
    test("Array global first", () => expect(items_first).toBe("item_0"));
    test("Array global last", () => expect(items_last).toBe("item_49"));
    test("global string mutated via fn", () =>
        expect(counter_len > 500).toBe(true));
});
