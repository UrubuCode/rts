// (#261/#216) `obj[k]` com k variavel — devolve o valor com o tipo REAL
// (number/string/function/...), NAO coerce eager pra string. JS spec:
// `o["age"]` eh o number 30, nao "30". A coercao p/ string acontece so'
// no contexto de uso (concat/template via TPL_COERCE_AUTO).

import { describe, test, expect } from "rts:test";

const o = { name: "Alice", age: 30, tag: "admin" };

// Acesso via var string
const k1: string = "name";
const k2: string = "age";
const k3: string = "tag";

const v1 = o[k1];
const v2 = o[k2];
const v3 = o[k3];

// Concat em template
const t1 = "hi " + o[k1];
const t2 = "id=" + o[k2];

// Sequencial unrolled (loop em string[] perde tipo da var no codegen
// atual — caveat documentado, nao parte deste fix).
let acc = "";
const ka: string = "name";
const kb: string = "age";
const kc: string = "tag";
acc = acc + o[ka] + ",";
acc = acc + o[kb] + ",";
acc = acc + o[kc] + ",";

// Computed em fn
function getField(obj: any, k: string): any {
    return obj[k];
}
const fnRes = getField(o, "name");

describe("computed_var_key", () => {
    test("o[k] string field", () => expect(v1).toBe("Alice"));
    // JS spec: o["age"] eh o number 30 (nao a string "30").
    test("o[k] number field", () => expect(v2).toBe(30));
    test("o[k] tag field", () => expect(v3).toBe("admin"));
    test("template hi + o[k]", () => expect(t1).toBe("hi Alice"));
    test("template id= + o[age]", () => expect(t2).toBe("id=30"));
    test("loop over keys", () => expect(acc).toBe("Alice,30,admin,"));
    test("fn arg computed", () => expect(fnRes).toBe("Alice"));
});
