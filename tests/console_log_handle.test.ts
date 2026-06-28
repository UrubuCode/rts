// (#573) console.log/template literal lidam com handles ambiguos:
// null/undefined, U64 raw (JSON.parse de scalar), handles invalidos.

import { describe, test, expect } from "rts:test";

let captured = "";
const origLog = console.log;
function captureLog(...args: any[]): void {
    let s = "";
    for (let i = 0; i < args.length; i++) {
        if (i > 0) s = s + " ";
        s = s + args[i];
    }
    captured = captured + s + "\n";
}

// Em vez de mexer com console.log direto (intercept costuma ser fragil),
// usamos template literal que segue o mesmo path COERCE_AUTO.

// 1. null em template literal -> "null"
const a = `[${null}]`;

// 2. undefined em template -> "undefined"
const b = `[${undefined}]`;

// 3. JSON.parse com scalar number
const c = `[${JSON.parse('42')}]`;

// 4. JSON.parse com string scalar
const d = `[${JSON.parse('"hello"')}]`;

// 5. JSON.parse com null
const e = `[${JSON.parse('null')}]`;

// 6. JSON.parse com bool — apos PR #1206 retorna sentinel JS (MIN/MIN+1)
// que TPL_COERCE_AUTO mapeia pra "true"/"false".
const f = `[${JSON.parse('true')}]`;

// 7. WeakMap.get retornando i64 (numero)
const wm = new WeakMap();
const k = {};
wm.set(k, 99);
const wmV = wm.get(k);
const g = `[${wmV}]`;

// 8. WeakMap.get com chave inexistente — retorna undefined em JS (handle 0)
const wmMiss = wm.get({});
const h = `[${wmMiss}]`;

// 9. obj literal stringificado — "[object Object]"
const o = { x: 1 };
const i_ = `[${o}]`;

// 10. Array stringificado — "1,2,3"
const arr = [1, 2, 3];
const j = `[${arr}]`;

describe("console_log_handle", () => {
    test("null in template literal", () => expect(a).toBe("[null]"));
    test("undefined in template literal", () => expect(b).toBe("[undefined]"));
    test("JSON.parse number scalar", () => expect(c).toBe("[42]"));
    test("JSON.parse string scalar", () => expect(d).toBe("[hello]"));
    test("JSON.parse null", () => expect(e).toBe("[null]"));
    test("JSON.parse true (PR #1206: sentinel → 'true')", () => expect(f).toBe("[true]"));
    test("WeakMap.get number value", () => expect(g).toBe("[99]"));
    test("WeakMap.get missing → undefined (igual a JS/Node)", () =>
        expect(h).toBe("[undefined]"));
    test("object literal stringify", () =>
        expect(i_).toBe("[[object Object]]"));
    test("array literal stringify", () => expect(j).toBe("[1,2,3]"));
});
