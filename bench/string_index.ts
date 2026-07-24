// Bench de INDEXAÇÃO DE STRING — o custo que define se um parser escrito em TS
// (JSON, tokenizador, CSV) é linear ou quadrático.
//
//   rts.exe run bench/string_index.ts
//
// Cada caso dobra o tamanho da entrada. Se o tempo QUADRUPLICAR de um caso para
// o seguinte, a operação é O(n) por acesso e o laço virou O(n²). Se apenas
// dobrar, está linear.
//
// Contexto: `s[i]`, `charAt`, `charCodeAt`, `substring`, `slice` e `.length`
// faziam `encode_utf16().collect()` da string INTEIRA a cada chamada. Varrer
// 98 K caracteres levava 20,7 s e um `JSON.parse` de 98 KB levava 42 s.
import io from "rts:io";

function mkAscii(n: number): string {
  // dobra o tamanho a cada passo (log2(n) concatenações, não n)
  let s = "0123456789abcdef";
  while (s.length < n) s = s + s;
  return s;
}

function scanCharCode(s: string): number {
  const n = s.length;
  let hits = 0;
  let i = 0;
  while (i < n) {
    if (s.charCodeAt(i) === 53) hits = hits + 1;
    i = i + 1;
  }
  return hits;
}

function scanIndex(s: string): number {
  const n = s.length;
  let hits = 0;
  let i = 0;
  while (i < n) {
    if (s[i] === "5") hits = hits + 1;
    i = i + 1;
  }
  return hits;
}

function sliceMany(s: string, count: number): number {
  // milhares de fatias curtas: o padrão de um parser extraindo tokens
  let total = 0;
  let i = 0;
  while (i < count) {
    const off = (i * 7) % (s.length - 12);
    total = total + s.substring(off, off + 8).length;
    i = i + 1;
  }
  return total;
}

io.print("== indexação de string (dobrar o tamanho deve DOBRAR o tempo) ==");
let size = 16384;
let step = 0;
while (step < 3) {
  const s = mkAscii(size);
  const a = scanCharCode(s);
  const b = scanIndex(s);
  const c = sliceMany(s, 4000);
  io.print("  n=" + s.length + " charCodeAt=" + a + " index=" + b + " slices=" + c);
  size = size * 2;
  step = step + 1;
}

io.print("== JSON.parse (o consumidor real dessas operações) ==");
{
  let obj = "{\"a\":1,\"b\":\"texto\",\"c\":[1,2,3]}";
  let arr = "[" + obj;
  let k = 0;
  while (k < 999) { arr = arr + "," + obj; k = k + 1; }
  arr = arr + "]";
  const parsed = JSON.parse(arr);
  io.print("  parseou " + parsed.length + " objetos de " + arr.length + " chars");
  const back = JSON.stringify(parsed);
  io.print("  stringify devolveu " + back.length + " chars");
}
io.print("== fim ==");
