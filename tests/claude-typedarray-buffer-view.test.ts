import { describe, test, expect } from "rts:test";

// `new T(buffer, byteOffset, length)` — a sobrecarga de VIEW sobre um
// ArrayBuffer, e a superfície que ela destrava.
//
// ANTES: os oito construtores de TypedArray tinham UM parâmetro só, então
// `new Uint8Array(buf, 4)` e `new Uint8Array(buf, 4, 3)` morriam em tempo de
// compilação com "no matching constructor" — um bail que mata o programa
// inteiro. Uma view existia (`new Uint8Array(buf)`), mas sempre cobria o buffer
// TODO: não havia janela.
//
// AGORA a view carrega `(byteOffset, length)` nos próprios slots
// (`taops/view.rs`), e um único corpo de construtor por tipo de elemento serve
// as seis sobrecargas do JS — `n` / `array` / `typedArray` / `buf` / `buf,off` /
// `buf,off,len` — distinguidas pelo TIPO do argumento em runtime, não por seis
// linhas registradas nem por um caminho por classe.
//
// Todos os valores abaixo foram conferidos contra o Node.

// ── Pré-computado no topo (chamar método de instância dentro do `test()` pode
// esbarrar no GC — ver CLAUDE.md "Testing creativity"). ──────────────────────

const buf = new ArrayBuffer(16);
const fill = new Uint8Array(buf);
for (let i = 0; i < 16; i++) fill[i] = i;

const w3 = new Uint8Array(buf, 4, 3);
const w3len = w3.length;
const w3at0 = w3[0];
const w3at2 = w3[2];
const w3at3 = w3[3];
const w3off = w3.byteOffset;
const w3buf = w3.buffer === buf;

const rest = new Uint8Array(buf, 4);
const restLen = rest.length;
const restAt0 = rest[0];

const i32v = new Int32Array(buf, 4, 2);
const i32len = i32v.length;
const i32at0 = i32v[0];
const i32bytes = i32v.byteLength;
const i32one = new Int32Array(buf, 8, 1)[0];
const f64len = new Float64Array(buf, 8, 1).length;
const u16v = new Uint16Array(buf, 2, 3);
const u16len = u16v.length;
const u16at0 = u16v[0];

// Aliasing: a janela compartilha os bytes com o buffer.
const b2 = new ArrayBuffer(8);
const va = new Uint8Array(b2, 2, 3);
va[0] = 99;
const aliasWhole = new Uint8Array(b2)[2];
const aliasSelf = va[0];
// Escrita FORA da janela é descartada (não vaza para o resto do buffer).
va[5] = 7;
const oobDropped = new Uint8Array(b2)[7];

// Iteração sobre uma VIEW (antes: "no such method on runtime class Uint8Array").
const vw = new Uint8Array(b2, 2, 3);
const vwValues = [...vw.values()].join(",");
const vwKeys = [...vw.keys()].join(",");
const vwEntriesLen = [...vw.entries()].length;
const vwEntry1 = [...vw.entries()][1].join(",");
let vwSum = 0;
for (const x of vw) vwSum += x;

// `subarray` sobre uma view: lê a JANELA (antes lia os slots de cabeçalho da
// view e devolvia lixo). É uma CÓPIA do intervalo, não uma sub-view viva — ver
// a DIVERGÊNCIA documentada em `taops/elems.rs::__rtsadp_arr_subarray`.
const b3 = new ArrayBuffer(8);
const w8 = new Uint8Array(b3);
for (let i = 0; i < 8; i++) w8[i] = i * 10;
const subLen = w8.subarray(2).length;
const subAt0 = w8.subarray(2)[0];
const subRangeLen = w8.subarray(2, 5).length;
const subLast = w8.subarray(2)[5];

// Construtor a partir de OUTRO typed array: cópia com re-wrap no tipo destino.
const fromI8 = new Uint8Array(new Int8Array([-1]))[0];
const fromU8 = new Int8Array(new Uint8Array([200]))[0];

// `view.set(src, offset)` respeitando a janela.
const b4 = new ArrayBuffer(6);
const v4 = new Uint8Array(b4, 1, 4);
v4.set([1, 2, 3], 1);
const setResult = Array.from(new Uint8Array(b4)).join(",");

describe("TypedArray view sobre ArrayBuffer (byteOffset, length)", () => {
  test("new Uint8Array(buf, 4, 3) é a janela de 3 elementos a partir do byte 4", () => {
    expect(w3len).toBe(3);
    expect(w3at0).toBe(4);
    expect(w3at2).toBe(6);
    // Índice além da JANELA é `undefined`, mesmo havendo bytes no buffer.
    expect(w3at3).toBe(undefined);
  });

  test("`length` omitido cobre o resto do buffer", () => {
    expect(restLen).toBe(12);
    expect(restAt0).toBe(4);
  });

  test("a janela é contada em ELEMENTOS, o offset em BYTES", () => {
    expect(i32len).toBe(2);
    expect(i32at0).toBe(117835012);
    expect(i32one).toBe(185207048);
    expect(f64len).toBe(1);
    expect(u16len).toBe(3);
    expect(u16at0).toBe(770);
  });

  test("byteOffset / byteLength / buffer", () => {
    expect(w3off).toBe(4);
    expect(i32bytes).toBe(8);
    expect(w3buf).toBe(true);
  });

  test("a janela COMPARTILHA os bytes; escrita fora dela é descartada", () => {
    expect(aliasWhole).toBe(99);
    expect(aliasSelf).toBe(99);
    expect(oobDropped).toBe(0);
  });
});

describe("TypedArray view — iteração e subarray", () => {
  test("values / keys / entries sobre uma view", () => {
    expect(vwValues).toBe("99,0,0");
    expect(vwKeys).toBe("0,1,2");
    expect(vwEntriesLen).toBe(3);
    expect(vwEntry1).toBe("1,0");
  });

  test("for..of anda só a janela", () => {
    expect(vwSum).toBe(99);
  });

  test("subarray de uma view lê a janela (comprimento e elementos certos)", () => {
    expect(subLen).toBe(6);
    expect(subAt0).toBe(20);
    expect(subLast).toBe(70);
    expect(subRangeLen).toBe(3);
  });

  test("view.set(src, offset) escreve dentro da janela", () => {
    expect(setResult).toBe("0,0,1,2,3,0");
  });
});

describe("TypedArray a partir de outro TypedArray", () => {
  test("copia com re-wrap no domínio do tipo destino", () => {
    expect(fromI8).toBe(255);
    expect(fromU8).toBe(-56);
  });
});
