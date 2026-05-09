// (#261) `obj["literal"]` agora propaga field type igual `obj.literal` —
// retorno de string field nao vem mais como handle bruto em template.

import { describe, test, expect } from "rts:test";

const o = { name: "Alice", age: 30, tag: "admin" };

// Acesso static vs computed string literal
const a1: string = o.name;
const a2: string = o["name"];
const b1: i64 = o.age;
const b2: i64 = o["age"];

// Concatenacao em template
const t1 = "hi " + o["name"];
const t2 = "id=" + o["age"];

// Mix
const t3 = o["name"] + ":" + o["age"];

// Multi-key
const u = o["tag"];

describe("computed_key_string_lit", () => {
    test("static .name", () => expect(a1).toBe("Alice"));
    test("computed [name]", () => expect(a2).toBe("Alice"));
    test("static .age", () => expect(b1).toBe(30));
    test("computed [age]", () => expect(b2).toBe(30));
    test("template hi + [name]", () => expect(t1).toBe("hi Alice"));
    test("template id= + [age]", () => expect(t2).toBe("id=30"));
    test("template [name]:[age]", () => expect(t3).toBe("Alice:30"));
    test("computed [tag]", () => expect(u).toBe("admin"));
});
