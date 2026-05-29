// (#450) Suporte minimo a `arguments` object em fn user nao-arrow.
// Implementacao injeta Vec<i64> com todos os params no prologue quando
// o body referencia o ident `arguments`. \`arguments.length\` retorna
// aridade da fn.

import { describe, test, expect } from "rts:test";

function f0(): i64 { return arguments.length; }
function f1(a: i64): i64 { return arguments.length; }
function f3(a: i64, b: i64, c: i64): i64 { return arguments.length; }

function withVal(x: i64, y: i64): i64 {
    let total: i64 = 0;
    total = total + arguments.length;
    return total + x + y;
}

const r0 = f0();
const r1 = f1(7);
const r3 = f3(10, 20, 30);
const wv = withVal(5, 10);

// (#299) Variadic real: fn SEM params declarados ve TODOS os args passados
// (antes via so' a aridade declarada = 0). Resolvido por rest param sintetico
// `...arguments` (expand_arguments_object), reusando a infra de rest_args.
function variadic() { return arguments.length; }
function vfirst() { return arguments[0]; }
function vsum() {
    let s: i64 = 0;
    for (let i = 0; i < arguments.length; i++) s = s + (arguments[i] as i64);
    return s;
}
const v0 = variadic();
const v1 = variadic(7);
const v5 = variadic(1, 2, 3, 4, 5);
const vf = vfirst(42, 99);
const vs = vsum(10, 20, 30);

describe("arguments_object", () => {
    test("f0 arguments.length = 0", () => expect(r0).toBe(0));
    test("f1 arguments.length = 1", () => expect(r1).toBe(1));
    test("f3 arguments.length = 3", () => expect(r3).toBe(3));
    test("withVal: length 2 + 5 + 10 = 17", () => expect(wv).toBe(17));
    // (#299) variadic — args reais, nao aridade declarada
    test("variadic length 0/1/5", () =>
        expect(v0 + "|" + v1 + "|" + v5).toBe("0|1|5"));
    test("variadic arguments[0]", () => expect(vf).toBe(42));
    test("variadic sum via loop", () => expect(vs).toBe(60));
});
