import { describe, test, expect } from "rts:test";
import { time } from "rts";

// `charCodeAt` tem de ser O(1) por chamada, não O(n) sobre a string.
//
// Duas armadilhas já corrigidas, ambas fazendo um laço de leitura virar O(n²):
//   1. `str_val(recv)` devolvia uma `String` PRÓPRIA — copiava a string inteira
//      a cada chamada (cem mil cópias de cem mil bytes num laço de 100 KB);
//   2. `utf16_unit_at` chamava `bytes.is_ascii()`, que percorre todos os bytes
//      antes de devolver UM caractere.
//
// Medido antes do fix: 100 mil chamadas sobre 100 KB levavam ~6.200 ms, contra
// ~0 ms de um laço aritmético equivalente. Depois: ~5 ms.
//
// `charCodeAt` em laço é o padrão de QUALQUER varredura léxica (o pré-passo de
// `<script>` do DOM é um caso), então a regressão aqui é cara e silenciosa —
// aparece como "compilar JS grande é lento", não como um teste vermelho.
//
// Pré-computado no top-level (regra do projeto: método dentro de test() pode
// perder handle pro GC).

// ── string de 100 KB ────────────────────────────────────────────────────────
let grande = "";
let g = 0;
while (g < 10000) { grande = grande + "abcdefghij"; g = g + 1; }
const nGrande = grande.length;

// baseline: laço aritmético do mesmo tamanho, sem tocar em string
let tBase = time.now_ms();
let soma = 0;
let b = 0;
while (b < nGrande) { soma = soma + b; b = b + 1; }
const msBase = time.now_ms() - tBase;

// o mesmo laço, lendo cada caractere
let tLeitura = time.now_ms();
let acc = 0;
let i = 0;
while (i < nGrande) { acc = acc + grande.charCodeAt(i); i = i + 1; }
const msLeitura = time.now_ms() - tLeitura;

// ── semântica UTF-16: tem de bater com o Node ──────────────────────────────
const ascii = "abc";
const asciiIni = ascii.charCodeAt(0);
const asciiFim = ascii.charCodeAt(2);
const asciiOob = ascii.charCodeAt(3);
const asciiNeg = ascii.charCodeAt(-1);

// multi-byte: índice de code unit NÃO coincide com índice de byte
const acentos = "áé";
const acento0 = acentos.charCodeAt(0);
const acento1 = acentos.charCodeAt(1);
const acentoOob = acentos.charCodeAt(2);

// ASCII e multi-byte MISTURADOS — o caso que um fast-path ingênuo erra
const misto = "aáb";
const misto0 = misto.charCodeAt(0);
const misto1 = misto.charCodeAt(1);
const misto2 = misto.charCodeAt(2);

// par surrogate: um emoji ocupa DUAS code units
const emoji = "a😀b";
const emojiLen = emoji.length;
const emoji0 = emoji.charCodeAt(0);
const emoji1 = emoji.charCodeAt(1);
const emoji2 = emoji.charCodeAt(2);
const emoji3 = emoji.charCodeAt(3);

const vazio = "";
const vazioOob = vazio.charCodeAt(0);

describe("charCodeAt — semântica UTF-16", () => {
  test("ASCII e fora de faixa", () => {
    expect(asciiIni).toBe(97);
    expect(asciiFim).toBe(99);
    expect(Number.isNaN(asciiOob)).toBe(true);
    expect(Number.isNaN(asciiNeg)).toBe(true);
    expect(Number.isNaN(vazioOob)).toBe(true);
  });

  test("multi-byte indexa por CODE UNIT, não por byte", () => {
    expect(acento0).toBe(225);
    expect(acento1).toBe(233);
    expect(Number.isNaN(acentoOob)).toBe(true);
  });

  test("ASCII e multi-byte misturados", () => {
    expect(misto0).toBe(97);
    expect(misto1).toBe(225);
    expect(misto2).toBe(98);
  });

  test("par surrogate conta como duas code units", () => {
    expect(emojiLen).toBe(4);
    expect(emoji0).toBe(97);
    expect(emoji1).toBe(55357);
    expect(emoji2).toBe(56832);
    expect(emoji3).toBe(98);
  });
});

describe("charCodeAt — custo", () => {
  test("ler 100 KB caractere a caractere não é O(n²)", () => {
    // Antes do fix: ~6.200 ms. Depois: ~5 ms. O teto de 2 s é folgado de
    // propósito — pega a regressão de ORDEM DE GRANDEZA sem ficar sensível a
    // máquina lenta ou build de debug.
    expect(msLeitura < 2000).toBe(true);
    // E não pode ser absurdamente mais caro que o laço vazio equivalente.
    expect(msLeitura < msBase + 2000).toBe(true);
  });
});
