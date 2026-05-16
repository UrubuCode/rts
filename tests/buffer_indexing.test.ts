import { describe, test, expect } from "rts:test";

// (#58) `bytes[i]` em Buffer (TextEncoder.encode output) retornava
// 0/null porque INDEX_GET_AUTO so' tratava Vec/String/Map. Buffer caia
// no fallback Map_GET que retorna 0. Fix: ramo Buf que extrai byte
// como i64.

const enc = new TextEncoder();
const bytes = enc.encode("hi");

const b0 = bytes[0];
const b1 = bytes[1];
const oob = bytes[10];

describe("Buffer indexing direto (#58)", () => {
  test("bytes[0] = 'h' code = 104", () => expect(b0).toBe(104));
  test("bytes[1] = 'i' code = 105", () => expect(b1).toBe(105));
  test("bytes[OOB] retorna 0/undefined-ish", () => expect(oob === 0 || (oob as any) === undefined).toBe(true));
});
