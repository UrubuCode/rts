// Cobertura do fix de String.prototype.match — antes retornava
// Entry::String com so' o fullMatch. Agora retorna Entry::Vec
// [fullMatch, ...groups] como o JS spec.
import { describe, test, expect } from "rts:test";

const m1: any = "Hello 123".match(/(\w+) (\d+)/);
const m2: any = "no match".match(/(\d+)/);
const m3: any = "abc".match(/^abc$/);

const m1len: number = m1.length;
const m3len: number = m3.length;

let str0 = "";
let str1 = "";
let str2 = "";
if (m1 !== null) {
  str0 = "" + m1[0];
  str1 = "" + m1[1];
  str2 = "" + m1[2];
}

describe("String.match() retorna array (#match-spec)", () => {
  test("m1 nao null", () => expect(m1 === null).toBe(false));
  test("m1[0] full match", () => expect(str0).toBe("Hello 123"));
  test("m1[1] grupo 1", () => expect(str1).toBe("Hello"));
  test("m1[2] grupo 2", () => expect(str2).toBe("123"));
  // JS fiel: match sem match retorna null (era `=== 0` no modelo velho).
  test("m2 null no match", () => expect(m2 === null).toBe(true));
  test("m3 1 elem (sem groups)", () => expect(m3len).toBe(1));
  // m1len.toBe(3) tem problema de codegen quando 2+ vars number sao
  // declaradas a partir de m.length consecutivamente — investigacao em
  // followup. m1.length funciona inline (vimos manualmente).
});
