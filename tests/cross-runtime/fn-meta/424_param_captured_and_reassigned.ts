// Cross-runtime: um PARÂMETRO capturado por closure E reatribuído.
//
// É como Babel/tsc transpilam um parâmetro com valor padrão:
//   function s(a, flag = false)  ->  function s(a, flag) { flag === void 0 && (flag = false); … }
// O param passa a ser reatribuído, então nenhuma closure que o captura pode
// levar uma cópia por valor. O RTS só sabia encaixotar (cell) um `let`, e um
// param mutado fazia o lifter RECUSAR a extração — o arquivo inteiro morria em
// "expression arrow". Agora o prólogo da própria função aloca a cell a partir
// do argumento recebido, que é a mesma regra do `let` (quem declara, aloca).

// 1. o padrão do parâmetro com default transpilado
function comDefault(a: number, flag?: boolean): string {
  flag === undefined && (flag = false);
  const ler = function (): string {
    return "flag=" + flag;
  };
  return ler();
}
console.log("default_omitido=" + comDefault(1, undefined));
console.log("default_dado=" + comDefault(1, true));

// 2. o param muda DEPOIS da closure existir — ela vê o valor novo
function mudaDepois(n: number): number {
  const ler = function (): number {
    return n;
  };
  n = n + 10;
  return ler();
}
console.log("muda_depois=" + mudaDepois(5));

// 3. a closure ESCREVE o param; o corpo externo enxerga
function closureEscreve(v: string): string {
  const set = function (x: string): void {
    v = x;
  };
  set("novo");
  return v;
}
console.log("closure_escreve=" + closureEscreve("velho"));

// 4. duas closures compartilham o MESMO param
function duasClosures(p: number): number {
  const inc = function (): void {
    p = p + 1;
  };
  const ler = function (): number {
    return p;
  };
  inc();
  inc();
  return ler();
}
console.log("duas_closures=" + duasClosures(0));

// 5. cada CHAMADA tem sua própria caixa (não vaza entre invocações)
function contador(inicio: number): () => number {
  return function (): number {
    inicio = inicio + 1;
    return inicio;
  };
}
const c1 = contador(0);
const c2 = contador(100);
console.log("c1=" + c1() + "," + c1());
console.log("c2=" + c2());

// 6. param mutado E lido dentro de um laço com closure por iteração
function noLaco(base: number): string {
  const fs: Array<() => number> = [];
  for (let i = 0; i < 3; i++) {
    fs.push(function (): number {
      return base + i;
    });
  }
  base = base * 10;
  return fs.map((f) => f()).join(",");
}
console.log("no_laco=" + noLaco(1));
