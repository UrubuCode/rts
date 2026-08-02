// Cross-runtime: um GENERATOR-EXPRESSÃO que ESCREVE uma variável capturada.
//
// É a forma que todo `async` transpilado produz: o `asyncToGenerator` do Babel
// embrulha um `function*` que memoiza requires preguiçosos (`s || (s = f())`).
//
// O RTS leva um generator-expressão para o topo (a state-machine só existe para
// declarações de topo) e passava as capturas como ARGUMENTOS — por valor. Uma
// escrita ficaria no parâmetro e nunca chegaria ao escopo de origem, então o
// motor RECUSAVA o arquivo inteiro. Agora as capturadas ESCRITAS viajam por
// referência (par getter/setter), e o lifter de closures faz da variável uma
// célula compartilhada.
//
// A fixture conta as inicializações: o modo de falha aqui é a memoização não
// "colar", o que produz o valor certo com a inicialização rodando toda vez.

function drive(it: any, send: string): string {
  let r = it.next();
  let out = "";
  while (!r.done) {
    out = out + "[y " + r.value + "]";
    r = it.next(send);
  }
  return out + "[ret " + r.value + "]";
}

function fabrica() {
  let cache: any;
  let inits = 0;
  const g = function* (x: number) {
    const m =
      cache ||
      (cache = (function () {
        inits = inits + 1;
        return { nome: "M" };
      })());
    return yield m.nome + "/" + x;
  };
  return {
    g: g,
    inits: function (): number {
      return inits;
    },
    cache: function (): any {
      return cache;
    },
  };
}

const o = fabrica();
console.log("primeira=" + drive(o.g(1), "A"));
console.log("segunda=" + drive(o.g(2), "B"));
console.log("inits=" + o.inits());
console.log("escapou=" + (o.cache() !== undefined));

// a escrita é visível para OUTRO consumidor do mesmo escopo.
// Usa yield em posição de VALOR (`const _ = yield x`) porque é essa forma que
// exige a state-machine — e é ela que o hoist por referência serve.
function contador() {
  let n = 0;
  const g = function* () {
    n = n + 1;
    const a = yield n;
    n = n + 10;
    const b = yield n;
    return a + "/" + b;
  };
  return {
    g: g,
    ler: function (): number {
      return n;
    },
  };
}
const c = contador();
console.log("passos=" + drive(c.g(), "s"));
console.log("visto_de_fora=" + c.ler());
