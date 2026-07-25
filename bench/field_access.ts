// Bench de ACESSO A CAMPO de instância — o custo que define quantos objetos um
// laço consegue percorrer por frame (motor de jogo, ECS, simulação).
//
//   rts.exe run bench/field_access.ts
//
// Dois eixos:
//   1) leitura de campo vs aritmética pura (mesmo nº de operações)
//   2) custo de ler 2 campos numa classe de 2, 5, 10 e 20 campos — se o tempo
//      CRESCE com o total de campos, a resolução do slot está varrendo a lista
//      de chaves da classe a cada acesso (era o caso: mutex global + clone de
//      uma String por campo + busca linear, ~1,4 µs por leitura).
import io from "rts:io";

const FRAMES = 180;
const N = 500;

class C2 { f0: f64; f1: f64;
  constructor() { this.f0 = 1.0; this.f1 = 2.0; } }
class C5 { f0: f64; f1: f64; f2: f64; f3: f64; f4: f64;
  constructor() { this.f0 = 1.0; this.f1 = 2.0; this.f2 = 0.0; this.f3 = 0.0; this.f4 = 0.0; } }
class C10 { f0: f64; f1: f64; f2: f64; f3: f64; f4: f64; f5: f64; f6: f64; f7: f64; f8: f64; f9: f64;
  constructor() { this.f0 = 1.0; this.f1 = 2.0; this.f2 = 0.0; this.f3 = 0.0; this.f4 = 0.0;
    this.f5 = 0.0; this.f6 = 0.0; this.f7 = 0.0; this.f8 = 0.0; this.f9 = 0.0; } }
class C20 { f0: f64; f1: f64; f2: f64; f3: f64; f4: f64; f5: f64; f6: f64; f7: f64; f8: f64; f9: f64;
  g0: f64; g1: f64; g2: f64; g3: f64; g4: f64; g5: f64; g6: f64; g7: f64; g8: f64; g9: f64;
  constructor() { this.f0 = 1.0; this.f1 = 2.0; this.f2 = 0.0; this.f3 = 0.0; this.f4 = 0.0;
    this.f5 = 0.0; this.f6 = 0.0; this.f7 = 0.0; this.f8 = 0.0; this.f9 = 0.0;
    this.g0 = 0.0; this.g1 = 0.0; this.g2 = 0.0; this.g3 = 0.0; this.g4 = 0.0;
    this.g5 = 0.0; this.g6 = 0.0; this.g7 = 0.0; this.g8 = 0.0; this.g9 = 0.0; } }

io.print("== leitura de campo vs aritmética (450k operações cada) ==");
{
  let f = 0;
  let acc = 0.0;
  while (f < FRAMES) {
    let i = 0;
    while (i < N) { acc = acc + 1.0 + 2.0 + 3.0 + 4.0 + 5.0; i = i + 1; }
    f = f + 1;
  }
  io.print("  aritmética pura  acc=" + acc);
}
{
  const arr: C5[] = [];
  let k = 0;
  while (k < N) { arr.push(new C5()); k = k + 1; }
  let f = 0;
  let acc = 0.0;
  while (f < FRAMES) {
    let i = 0;
    while (i < N) {
      const o = arr[i];
      acc = acc + o.f0 + o.f1 + o.f2 + o.f3 + o.f4;
      i = i + 1;
    }
    f = f + 1;
  }
  io.print("  5 campos lidos   acc=" + acc);
}

io.print("== ler 2 campos: o custo deve ser IGUAL nas 4 classes ==");
{
  const a2: C2[] = []; const a5: C5[] = []; const a10: C10[] = []; const a20: C20[] = [];
  let k = 0;
  while (k < N) { a2.push(new C2()); a5.push(new C5()); a10.push(new C10()); a20.push(new C20()); k = k + 1; }
  let acc = 0.0;
  let f = 0;
  while (f < FRAMES) {
    let i = 0;
    while (i < N) { const o = a2[i]; acc = acc + o.f0 + o.f1; i = i + 1; }
    f = f + 1;
  }
  io.print("  classe de  2 campos  acc=" + acc);
  acc = 0.0; f = 0;
  while (f < FRAMES) {
    let i = 0;
    while (i < N) { const o = a5[i]; acc = acc + o.f0 + o.f1; i = i + 1; }
    f = f + 1;
  }
  io.print("  classe de  5 campos  acc=" + acc);
  acc = 0.0; f = 0;
  while (f < FRAMES) {
    let i = 0;
    while (i < N) { const o = a10[i]; acc = acc + o.f0 + o.f1; i = i + 1; }
    f = f + 1;
  }
  io.print("  classe de 10 campos  acc=" + acc);
  acc = 0.0; f = 0;
  while (f < FRAMES) {
    let i = 0;
    while (i < N) { const o = a20[i]; acc = acc + o.f0 + o.f1; i = i + 1; }
    f = f + 1;
  }
  io.print("  classe de 20 campos  acc=" + acc);
}
io.print("== fim ==");
