// Cross-runtime: um LOCAL sombreia uma função de topo de mesmo nome (escopo JS).
//
// Minificador reusa letras (`e`, `t`, `n`, `r`) em todo módulo, então essa
// colisão é a regra, não a exceção. No RTS o lifter de closures tratava um nome
// livre como "é a função de topo homônima" ANTES de olhar o escopo léxico, e a
// captura era descartada: a função aninhada lia a FUNÇÃO em vez do local, e a
// escrita nunca chegava ao local. Resultado: valor errado EM SILÊNCIO (nenhum
// erro), que é pior do que uma recusa.

// função de topo chamada `e`
var e = function (): string {
  return "TOPO";
};

// fábrica com o SEU PRÓPRIO `e`, capturado E escrito por uma função aninhada
// (memoização de require preguiçoso — a forma que todo bundle da Meta usa)
function fabrica(req: (n: string) => { id: string }): () => string {
  var e: { id: string } | undefined,
    s: string | null = null;
  function pega(): string {
    if (s != null) return s;
    const v = (e || (e = req("Env"))).id;
    s = "sess-" + v;
    return s;
  }
  return pega;
}

const p = fabrica(function (n: string) {
  return { id: "ID:" + n };
});
console.log("primeira=" + p());
console.log("memoizada=" + p());
console.log("topo_intacto=" + e());

// mesmo nome, agora como PARÂMETRO sombreando a função de topo
function comParam(e: string): string {
  const inner = function (): string {
    return "param:" + e;
  };
  return inner();
}
console.log("param=" + comParam("X"));

// local sombreando, escrito por DUAS closures irmãs (a mesma célula)
function duasIrmas(): string {
  var e = 0;
  const inc = function (): void {
    e = e + 1;
  };
  const ler = function (): number {
    return e;
  };
  inc();
  inc();
  return "cont=" + ler();
}
console.log("irmas=" + duasIrmas());

// o local sombreia mesmo quando NUNCA escrito (só lido)
function soLeitura(): string {
  const e = "local";
  const f = function (): string {
    return e;
  };
  return f();
}
console.log("so_leitura=" + soLeitura());
