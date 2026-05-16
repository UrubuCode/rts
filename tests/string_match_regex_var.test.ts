import { describe, test, expect } from "rts:test";

// (#39) `text.match(regexVar)` retornava null porque o codegen so'
// detectava regex literal direto (`text.match(/pat/)`), nao ident
// de var registrada como RegExp. Fix: marca var inicializada com
// regex literal como local_class_ty="RegExp" e detecta no
// lower_string_builtin do `match`.

const text = "Item-42";
const basic = /item-(\d+)/i;
const m = text.match(basic);
const m0 = m ? m[0] : "none";
const m1 = m ? m[1] : "none";

// Literal direto continua funcionando.
const lit = text.match(/(\d+)/);
const lit0 = lit ? lit[0] : "none";

describe("String.match com regex em var (#39)", () => {
  test("text.match(regexVar) retorna match", () => expect(m0).toBe("Item-42"));
  test("text.match(regexVar) captura grupo", () => expect(m1).toBe("42"));
  test("text.match(/.../) literal continua OK", () => expect(lit0).toBe("42"));
});
