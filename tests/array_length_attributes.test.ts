// O descritor de `length` de um array e {writable, !enumerable, !configurable}
// para TODO array, e nao ha registro por celula que o diga: desde 2026-08-26
// `Context::implied_attributes` responde os tres a partir do fato de que a
// celula esta em `array_elements`. Escrever o registro custava 84 ns dos
// 136.6 de um `[]`.
//
// O que este arquivo fixa e a consequencia observavel de tirar a escrita, e
// nao o desempenho: um array recem-criado tem que responder identico a um que
// nunca passou por `defineProperty`, e um que passou tem que responder o que o
// programa mandou — porque agora o registro so existe quando ha desvio, e um
// desvio esquecido nao seria lentidao, seria `length` aparecendo em
// `Object.keys`.
import { describe, test, expect } from "rts:test";

const fresh = [1, 2, 3];
const pushed: number[] = [];
pushed.push(1);

const declared = { writable: false } as PropertyDescriptor;
const restricted = [1, 2];
Object.defineProperty(restricted, "length", declared);

const frozen = Object.freeze([1, 2]);

function shape(a: unknown[]): string {
  const d = Object.getOwnPropertyDescriptor(a, "length") as PropertyDescriptor;
  return [d.value, d.writable, d.enumerable, d.configurable].join("|");
}

describe("array length attributes without a per-cell record", () => {
  test("a fresh array", () => expect(shape(fresh)).toBe("3|true|false|false"));
  // `push` passava pelo mesmo `set_length`; se o registro voltasse a ser
  // escrito por ali, este e o array em que ele apareceria primeiro.
  test("after push", () => expect(shape(pushed)).toBe("1|true|false|false"));
  test("length is not enumerable", () =>
    expect(Object.keys(fresh).join(",")).toBe("0,1,2"));
  test("but it is an own name", () =>
    expect(Object.getOwnPropertyNames(fresh).join(",")).toBe("0,1,2,length"));
  test("for-in skips it", () => {
    const seen: string[] = [];
    for (const k in fresh) seen.push(k);
    expect(seen.join(",")).toBe("0,1,2");
  });
  // O desvio gravado tem que ganhar do implicito, nos dois sentidos: o
  // `writable: false` vale, e os outros dois continuam sendo os do array.
  test("a recorded deviation wins", () =>
    expect(shape(restricted)).toBe("2|false|false|false"));
  test("and it holds", () => {
    let threw = false;
    try {
      restricted.push(9);
    } catch {
      threw = true;
    }
    expect(threw && restricted.length === 2).toBe(true);
  });
  test("freeze reaches it too", () => expect(shape(frozen)).toBe("2|false|false|false"));
  // O controle: `length` de uma funcao e de um objeto comum NAO e o do array,
  // e a comparacao de chave em `implied_attributes` e o que separa os dois.
  test("a plain object's length is ordinary", () =>
    expect(Object.keys({ length: 3 }).join(",")).toBe("length"));
  test("a function's length is not enumerable either", () =>
    expect(Object.keys(shape).join(",")).toBe(""));
});
