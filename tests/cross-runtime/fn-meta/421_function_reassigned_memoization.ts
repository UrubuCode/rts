// Cross-runtime: uma DECLARAÇÃO de função cria um binding GRAVÁVEL, e
// reatribuí-lo é a memoização que todo async transpilado por Babel emite:
//
//   function v(a) { v = _asyncToGenerator(...); return v.apply(this, args) }
//
// O RTS resolvia o nome pela tabela imutável de funções de topo, então a
// escrita não tinha onde pousar e o arquivo inteiro era recusado.

function make() {
  return function (a: number) {
    return "real:" + a;
  };
}

// 1. a função troca a si mesma na primeira chamada
function v(a: number): string {
  v = make() as typeof v;
  return (v as (a: number) => string).apply(null, [a]);
}
console.log("self1=" + v(1));
console.log("self2=" + v(2));

// 2. outra função enxerga o valor NOVO (não a captura do original)
function chama(a: number): string {
  return (v as (a: number) => string).apply(null, [a]);
}
console.log("via_outra=" + chama(3));

// 3. reatribuição a partir do TOPO alcança quem já referenciava o nome
function base(): string {
  return "orig";
}
function leBase(): string {
  return base();
}
console.log("antes=" + leBase());
base = function (): string {
  return "trocada";
};
console.log("depois=" + leBase());

// 4. hoisting: o binding vale acima da própria linha de declaração
console.log("hoisted=" + antesDaDecl());
function antesDaDecl(): string {
  return "ok";
}

// 5. uma função NUNCA reatribuída não muda de comportamento
function pura(x: number): number {
  return x * 2;
}
console.log("pura=" + pura(21));
console.log("pura_name=" + pura.name);

// 6. reatribuição condicional (a forma que o wrapper realmente usa)
let contadorInit = 0;
function lazy(n: number): number {
  contadorInit = contadorInit + 1;
  lazy = function (m: number): number {
    return m + 100;
  };
  return lazy(n);
}
console.log("lazy1=" + lazy(1));
console.log("lazy2=" + lazy(2));
console.log("init_rodou=" + contadorInit);
