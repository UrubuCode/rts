import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#345) Tagged template cuja tag faz `strings.reduce(cb, "")` onde o
// callback inline CAPTURA o rest param `...values`. O lift reescreve para
// `strings.reduce_bound(handle, "", ...)` e a inferencia de return type
// precisa reconhecer `reduce_bound` com init string => fn retorna string
// (Handle), nao Number (f64) — senao o handle vira lixo no console.log.
function highlight(strings: TemplateStringsArray, ...values: any[]) {
  return strings.reduce(
    (acc, str, i) => acc + str + (values[i] !== undefined ? "[" + values[i] + "]" : ""),
    "",
  );
}

const name = "world";
const n = 42;
const r1 = highlight`hello ${name} the number is ${n}`;
const r2 = highlight`no interpolation`;
const r3 = highlight`${1}${2}${3}`;

// reduce sem ternario (no-capture path) continua funcionando.
function plain(strings: TemplateStringsArray, ...vals: any[]) {
  return strings.reduce((acc, s) => acc + s, "");
}
const r4 = plain`a${1}b${2}c`;

// spread sobre TemplateStringsArray.
function parts(strings: TemplateStringsArray, ...vals: any[]) {
  return [...strings];
}
const r5 = parts`x${1}y`.join("|");

describe("tagged_template_reduce_capture (#345)", () => {
  test("tag com capture + ternario", () =>
    expect(r1).toBe("hello [world] the number is [42]"));
  test("tag sem interpolacao", () => expect(r2).toBe("no interpolation"));
  test("tag so interpolacao", () => expect(r3).toBe("[1][2][3]"));
  test("tag plain reduce", () => expect(r4).toBe("abc"));
  test("spread sobre strings", () => expect(r5).toBe("x|y"));
});
