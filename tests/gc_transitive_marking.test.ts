// (#398) GC scanner agora propaga marca transitiva atraves de
// Map.values, Vec.elements e Proxy.{target,handler}. Sem isto,
// handles armazenados dentro de outro handle eram swept enquanto
// o pai ainda estava vivo (use-after-free).

import { describe, test, expect } from "rts:test";

// 1. Map com chaves estaticas + values string handle. Antes do
//    transitive marking, GC sweep coletava o string handle armazenado
//    no slot; depois disso, leitura via member access retornava lixo.
const m1: any = { a: "value_a", b: "value_b", c: "value_c" };
for (let j = 0; j < 1000; j++) {
    const dummy = "tmp_" + j;
}
const acc1 = m1.a + "|" + m1.b + "|" + m1.c;

// 2. Array de strings — handle Vec contem handles String.
const arr2: string[] = [];
for (let i = 0; i < 30; i++) {
    arr2.push("item_" + i);
}
for (let j = 0; j < 1000; j++) {
    const dummy = "x_" + j;
}
const arr2_first = arr2[0];
const arr2_last = arr2[29];
const arr2_len = arr2.length;

// 3. Map de Map (nested handle) — entry interno tambem precisa
//    sobreviver ao sweep do entry externo.
const outer: any = {};
outer["inner"] = { name: "preserved" };
for (let j = 0; j < 1000; j++) {
    const dummy = "y_" + j;
}
const innerName = outer["inner"].name;

// 4. Vec de Map — elementos sao Map handles.
const records: any[] = [];
for (let i = 0; i < 20; i++) {
    records.push({ id: i, label: "rec_" + i });
}
for (let j = 0; j < 1000; j++) {
    const dummy = "z_" + j;
}
const rec0Id: i64 = records[0].id;
const rec19Label = records[19].label;

// 5. Proxy keep-alive — target e handler nao podem ser coletados
//    enquanto proxy estiver vivo.
const target5: any = { value: "alive" };
const handler5: any = {};
const proxy5: any = new Proxy(target5, handler5);
for (let j = 0; j < 1000; j++) {
    const dummy = "p_" + j;
}
// Acesso via proxy depende de target sobreviver.
const proxyVal = proxy5.value;

describe("gc_transitive_marking", () => {
    test("Map values (string handles) survive GC", () =>
        expect(acc1).toBe("value_a|value_b|value_c"));
    test("Vec of strings — first survives", () =>
        expect(arr2_first).toBe("item_0"));
    test("Vec of strings — last survives", () =>
        expect(arr2_last).toBe("item_29"));
    test("Vec of strings — length stable", () =>
        expect(arr2_len).toBe(30));
    test("Map of Map (nested handle) survives", () =>
        expect(innerName).toBe("preserved"));
    test("Vec of Map — id field", () => expect(rec0Id).toBe(0));
    test("Vec of Map — label string", () =>
        expect(rec19Label).toBe("rec_19"));
    test("Proxy target survives via transitive marking", () =>
        expect(proxyVal).toBe("alive"));
});
