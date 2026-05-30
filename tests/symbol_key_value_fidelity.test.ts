import { describe, test, expect } from "rts:test";

// (#216) Ler `obj[symbolKey]` / `obj[varKey]` preserva o TIPO REAL do
// valor (number/function/string), em vez de coerce eager pra string.
// Antes TPL_COERCE_VEC_SLOT convertia tudo p/ string handle, quebrando
// `typeof obj[k]`, chamada de funcao armazenada, e identidade numerica.

const s = Symbol("k");

// number sob chave symbol
const oNum: any = {};
oNum[s] = 42;
const numVal = oNum[s];
const numType: string = typeof numVal;

// function sob chave symbol — armazena e chama
const oFn: any = {};
oFn[s] = (a: number, b: number) => a + b;
const fnVal = oFn[s];
const fnCall = fnVal(10, 5);

// string sob chave symbol
const oStr: any = {};
oStr[s] = "hello";
const strVal = oStr[s];

// number sob chave string-var continua coercivel em concat
const oo: any = { age: 30 };
const k: string = "age";
const concat = "id=" + oo[k];

describe("symbol_key_value_fidelity (#216)", () => {
  test("number sob symbol key mantem tipo", () => expect(numType).toBe("number"));
  test("number sob symbol key mantem valor", () => expect(numVal).toBe(42));
  test("function sob symbol key eh chamavel", () => expect(fnCall).toBe(15));
  test("string sob symbol key", () => expect(strVal).toBe("hello"));
  test("number sob var key coerce em concat", () => expect(concat).toBe("id=30"));
});
