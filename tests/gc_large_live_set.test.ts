import { describe, test, expect } from "rts:test";

// O marcador do GC tinha um teto de 1.000.000 de passos, posto como "guarda contra
// ciclos patológicos". Ele era necessário só porque `mark` re-enumerava os filhos
// de um nó JÁ marcado, então um ciclo nunca terminava. O preço era truncar a
// marcação de qualquer conjunto VIVO grande: a cauda não marcada era varrida e a
// leitura devolvia `NaN` — resposta errada, em silêncio, em código comum.
//
// A terminação agora vem do bit de marca (cada slot é expandido no máximo uma vez
// por ciclo), e o teto foi removido.
//
// O tamanho aqui NÃO é arbitrário e não pode encolher: o GC periódico só dispara
// quando o conjunto vivo passa de GC_LIVE_FLOOR (500k handles), então é exatamente
// aí que a primeira marcação acontece e é exatamente aí que o teto era atingido.
// Medido no binário anterior: 450k passa, 500k devolve `NaN`.

class P { x: number; constructor(x: number) { this.x = x; } }

const N = 520000;
const keep: P[] = [];
for (let i = 0; i < N; i++) keep.push(new P(i));
let sum = 0;
for (let i = 0; i < N; i++) sum += keep[i].x;

describe("GC marks a large live set completely", () => {
  test("every one of 520k live objects survives marking", () => {
    expect(sum).toBe(((N - 1) * N) / 2);
  });
});
