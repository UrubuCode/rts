// Cobertura do fix de Bool em string concat / template literal.
// Bug raiz: emit_str_handle alocava 2 handles "true"/"false" + select,
// mas o handle selecionado era freed apos a expressao via
// register_temp_handle. Em segunda expressao, select pegava handle ja
// liberado -> string vazia.
// Fix: usa __RTS_FN_GL_BOOLEAN_TO_STRING (single call) ao inves de
// double-alloc + select.
import { describe, test, expect } from "rts:test";

let __out: string = "";
function out(s: string): void { __out += s + "\n"; }

const t: boolean = true;
const f: boolean = false;

// Cenario que reproduzia o bug: bool var seguido de bool literal.
out("t=" + t);
out("f=" + f);
out("plain false: " + false);
out("plain true: " + true);

// Combinacoes que retornam bool e devem stringify corretamente.
const arr = [1, 2, 3];
const isArr: boolean = Array.isArray(arr);
const everyPos: boolean = arr.every((n: number) => n > 0);
const everyNeg: boolean = arr.every((n: number) => n < 0);
const someBig: boolean = arr.some((n: number) => n > 10);
out("isArray=" + isArr);
out("every>0=" + everyPos);
out("every<0=" + everyNeg);
out("some>10=" + someBig);

// Multiplas Bool concat na mesma expressao.
out("multi: " + t + "/" + f + "/" + true + "/" + false);

const expected =
  "t=true\nf=false\nplain false: false\nplain true: true\n" +
  "isArray=true\nevery>0=true\nevery<0=false\nsome>10=false\n" +
  "multi: true/false/true/false\n";

describe("Bool em string concat — sem free prematuro", () => {
  test("bool var + literal nao perde handle", () => expect(__out).toBe(expected));
});
