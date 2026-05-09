// (#383) Documenta que ident desconhecido eh hard-fail. Como o teste
// nao pode chamar a fn (impede compile), validamos via meta-shape: o
// codigo eh well-typed e roda corretamente em todos os sitios onde
// o codegen antes acharia ident sem destino e segfault'aria.

import { describe, test, expect } from "rts:test";
import { io, math } from "rts";

// 1. Idents validos resolvem normalmente (regressao pos-fix #383)
const x = math.PI;
const y = math.sqrt(16);

// 2. User fn declarada chama bem
function userFn(a: i64): i64 { return a * 2; }
const ufn = userFn(7);

// 3. Builtin namespace dispatch funciona
const sq = math.pow(3, 2);

// 4. Class method dispatch funciona
class Box {
    v: i64;
    constructor(v: i64) { this.v = v; }
    sum(o: Box): i64 { return this.v + o.v; }
}
const b1 = new Box(10);
const b2 = new Box(20);
const sumBoxes: i64 = b1.sum(b2);

// 5. Member access via namespace funciona
const s = "hello";
const upper = s.toUpperCase();

describe("undefined_ident_hard_error_smoke", () => {
    test("math constants", () => expect(x).toBe(3.141592653589793));
    test("math.sqrt", () => expect(y).toBe(4));
    test("user fn call", () => expect(ufn).toBe(14));
    test("math.pow", () => expect(sq).toBe(9));
    test("class method", () => expect(sumBoxes).toBe(30));
    test("string method", () => expect(upper).toBe("HELLO"));
});
