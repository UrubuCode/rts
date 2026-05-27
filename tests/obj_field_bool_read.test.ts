import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): ler um campo `bool` de object literal cujo
// valor eh `false` retornava `true`. Causa: obj literal armazena bool como
// sentinel (i64::MIN = false, i64::MIN+1 = true), mas o map_get tipado
// devolvia o sentinel bruto tagueado como Bool — o resto do codegen espera
// Bool == 0/1, entao `false` (i64::MIN, nao-zero) coercia para "true".
// Fix: decodificar o sentinel de volta para 0/1 na leitura.

let out = "";
function print(v: string): void { out += v + "\n"; }

const o = { a: true, b: false };
print("" + o.a);          // true
print("" + o.b);          // false (era "true")
const viaDot = o.b;
print("" + viaDot);       // false

// destructuring (desugara para member access)
const { a, b } = o;
print("" + a);            // true
print("" + b);            // false

// usar bool de campo em condicional
print(o.b ? "yes" : "no"); // no
print(o.a ? "yes" : "no"); // yes

// padrao generator-like { value, done }
const r = { value: 42, done: false };
const { value, done } = r;
print("" + value + ":" + done); // 42:false

describe("obj field bool read", () => {
  test("false nao vira true", () =>
    expect(out).toBe("true\nfalse\nfalse\ntrue\nfalse\nno\nyes\n42:false\n"));
});
