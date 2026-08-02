// Cross-runtime: compound-assign e `++`/`--` sobre um alvo com PARTE EFETIVA.
//
// `o.p += v` é desugarado para `o.p = o.p + v`, então o alvo é lido E escrito.
// Se a base (ou a chave de índice) for uma chamada, ela rodaria DUAS vezes. O
// motor recusava esses alvos; agora cada parte efetiva é avaliada UMA vez num
// temporário e o desugar replica só identificadores.
//
// A fixture conta as chamadas justamente porque o modo de falha aqui é
// silencioso: o valor final pode sair certo enquanto o efeito colateral roda
// duas vezes.

let chamadas = 0;
const store: any = { a: { n: 1 }, arr: [10, 20] };

function pega(): any {
  chamadas = chamadas + 1;
  return store.a;
}
function idx(): number {
  chamadas = chamadas + 1;
  return 1;
}

pega().n += 5;
console.log("base_chamada=" + store.a.n + " chamadas=" + chamadas);

store.arr[idx()] *= 3;
console.log("chave_chamada=" + store.arr[1] + " chamadas=" + chamadas);

pega().n++;
console.log("incdec_base=" + store.a.n + " chamadas=" + chamadas);

store.arr[idx()]--;
console.log("incdec_chave=" + store.arr[1] + " chamadas=" + chamadas);

// base E chave efetivas ao mesmo tempo
function obj(): any {
  chamadas = chamadas + 1;
  return store;
}
obj().arr[idx()] += 100;
console.log("ambas=" + store.arr[1] + " chamadas=" + chamadas);

// o valor da expressão continua sendo o NOVO valor
console.log("valor=" + (pega().n += 1));
