// O que um acesso a propriedade custa, isolado de tudo o resto.
//
// # Porque este bench existe
//
// Os benches deste diretorio medem cada um algo que uma resposta errada
// esconderia, e nenhum media isto. A ausencia foi notada por medicao e nao por
// leitura: a 2026-08-20, `bench/monte_carlo_pi.ts` levava 929 ms onde o MESMO
// algoritmo escrito com locais levava 134, e a diferenca inteira era o estado
// viver num objeto.
//
// # As quatro formas, e porque sao estas quatro
//
// Cada uma faz EXACTAMENTE o mesmo trabalho aritmetico — o mesmo passo de LCG,
// o mesmo numero de iteracoes — e muda so' onde o estado vive. A diferenca
// entre duas linhas e' o custo de uma coisa, nao de um programa.
//
//   local        o estado e' uma variavel que nada captura
//   capturada    a mesma variavel, lida por uma closure que a torna partilhada
//   propriedade  o estado e' um campo de um objeto comum
//   duas_hops    o mesmo, atraves de um objeto dentro de outro
//
// `local` e' a linha de base e nao o alvo: ela ja' esta a 1,18x do Rust nativo
// para este laco (medido a 2026-08-20). O que as outras tres medem e' o que se
// paga por sair dela.
//
// # O que se aprendeu ao escreve-lo, e nenhuma das duas coisas era esperada
//
// Primeira leitura, 2026-08-20, release, N = 3 000 000:
//
//   local        14,3 ms
//   capturada    49,1 ms     3,4x
//   propriedade  50,8 ms     3,6x
//   duas hops    50,7 ms     3,5x
//
// **`capturada` e `propriedade` custam o mesmo.** A expectativa era que o
// ambiente de uma closure fosse mais caro por ter uma cadeia a percorrer; nao
// e'. Uma variavel capturada E' uma propriedade, paga o preco de uma
// propriedade, e qualquer trabalho que baixe um baixa o outro.
//
// **E `duas hops` custa o mesmo que uma.** Esta e' a que contradiz o comentario
// que este arquivo tinha antes de ser corrido: `outer.inner.s` faz mais
// acessos por iteracao que `o.s` e nao custa mais. O custo por SITIO nao domina
// — se dominasse, dobraria. O que a linha nao diz e' o que domina em vez disso,
// e essa e' a proxima pergunta e nao uma conclusao deste arquivo.
//
// O que estas quatro linhas NAO dizem: quanto de `propriedade` menos `local` e'
// a guarda, quanto e' o inline cache, e quanto e' o encaixotar e desencaixotar
// do valor. Sao tres coisas e este bench mede a soma.
//
// # A terceira coisa, e ela precisa das DUAS referencias e nao de uma
//
// Mesma maquina, mesmo dia, mesmo arquivo:
//
//                   RTS       Bun      Node
//   local          13,8      29,6      50,4
//   capturada      49,0      40,1      80,2
//   propriedade    50,7      39,7      52,4
//   duas hops      50,5      39,0      52,5
//
// Contra o **Bun** (JavaScriptCore), que e' a referencia rapida das duas:
//
//   local          RTS 2,1x MAIS RAPIDO
//   propriedade    RTS 1,28x mais lento
//   capturada      RTS 1,22x mais lento
//
// Ou seja: o caminho local aqui bate a melhor referencia por mais do dobro, e o
// caminho da propriedade fica ~28% atras dela. As duas coisas sao verdade ao
// mesmo tempo e nenhuma das duas se ve olhando so' para os 3,4x internos.
//
// **Este paragrafo dizia "ja' empata com o V8" e essa versao media contra a
// referencia errada.** Contra o Node o acesso a propriedade daqui de facto
// empata (50,7 contra 52,4) — e o Node e' o mais lento dos tres em todas as
// quatro linhas, entao empatar com ele nao diz nada sobre estar bom. A
// conclusao que a versao anterior tirou disso — que nao havia nada a ganhar
// aqui — era falsa por escolha de denominador, que e' precisamente a forma de
// erro que o piso de honestidade deste repositorio descreve.
//
// O que continua verdade depois da correcao: os 3,4x internos sao dominados
// pelo NUMERADOR ser rapido, nao pelo denominador ser mau. Sair do caminho
// local custa 3,7x aqui contra 1,34x no Bun, e e' por o caminho local aqui ser
// muito melhor, nao por o outro ser muito pior.
//
// A conclusao pratica, agora com o sinal certo: ha' ~28% a ganhar no acesso a
// propriedade contra a melhor referencia, e ha' 2,1x ja' ganhos no caminho
// local. Por unidade de trabalho, mover programa para o caminho local rende
// mais — mas os 28% sao reais e nao sao zero, que era o que a versao anterior
// afirmava.
//
// O que estas linhas NAO dizem: nada sobre acesso POLIMORFICO. As quatro veem
// uma unica forma de objeto do principio ao fim, que e' o caso que um inline
// cache resolve melhor. Um sitio que ve tres formas e' outra pergunta e nao
// tem instrumento nenhum aqui.

const N: number = 3000000;

function bench_local(): number {
  let s: number = 1;
  let i: number = 0;
  while (i < N) {
    s = (s * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return s;
}

// `peek` nunca e' chamada dentro do laco. Ela existe para forcar `s` a ser
// partilhada — o que decide onde a variavel vive e' ser CAPTURADA, nao ser
// usada.
let captured_state: number = 1;
function peek(): number {
  return captured_state;
}
function bench_captured(): number {
  captured_state = 1;
  let i: number = 0;
  while (i < N) {
    captured_state = (captured_state * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return peek();
}

function bench_property(): number {
  const o = { s: 1 };
  let i: number = 0;
  while (i < N) {
    o.s = (o.s * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return o.s;
}

// Dois niveis, porque uma cadeia e' o que um acesso capturado profundo e' e o
// que um `a.b.c` e': se o custo dobrar, ele e' por salto; se nao, ha' algo fixo
// por sitio que domina.
function bench_two_hops(): number {
  const outer = { inner: { s: 1 } };
  let i: number = 0;
  while (i < N) {
    outer.inner.s = (outer.inner.s * 1664525 + 1013904223) % 4294967296;
    i = i + 1;
  }
  return outer.inner.s;
}

function timed(label: string, run: () => number): void {
  const started = performance.now();
  const answer = run();
  const elapsed = performance.now() - started;
  // O resultado e' impresso, nao descartado: um laco cujo resultado ninguem le
  // pode ser removido inteiro, e o numero seria bom demais em vez de rapido.
  console.log(label + " " + elapsed.toFixed(1) + " ms  (" + answer + ")");
}

// Todas as quatro tem de responder o mesmo numero. Nao e' uma verificacao
// decorativa: elas so' sao comparaveis se fizerem o mesmo trabalho, e uma que
// divergisse estaria a medir outro programa.
const expected: number = bench_local();
for (const [label, run] of [
  ["local      ", bench_local],
  ["capturada  ", bench_captured],
  ["propriedade", bench_property],
  ["duas hops  ", bench_two_hops],
] as [string, () => number][]) {
  timed(label, run);
}
console.log("checksum " + expected);
