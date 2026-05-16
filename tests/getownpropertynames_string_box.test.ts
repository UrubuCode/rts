import { describe, test, expect } from "rts:test";

// (#789) Object.getOwnPropertyNames(new String("...")) deve retornar
// ["0","1",...,"length"] usando UTF-16 code units. Antes do fix, faltava
// o ramo Entry::String em OBJECT_OWN_PROPERTY_NAMES -> retornava vazio.

const s = new String("hi");
const names = (Object.getOwnPropertyNames(s) as any).sort().join(",");

const empty = new String("");
const emptyNames = (Object.getOwnPropertyNames(empty) as any).join(",");

describe("getOwnPropertyNames em String box (#789)", () => {
  test("'hi' -> 0,1,length", () => expect(names).toBe("0,1,length"));
  test("string vazia -> so length", () => expect(emptyNames).toBe("length"));
});
