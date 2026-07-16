// Cross-runtime: UMA coisa — parâmetro de função É um binding mutável comum, e
// uma closure que o captura enxerga/aplica mutações nele (não recebe uma cópia).
// Distinto de claude-closure-mutates-captured (aquele captura um `let` do corpo).
// Variações: closure muta o param; corpo muta e closure lê; param default;
// rest param; destructured param; param reusado como acumulador; múltiplas
// closures sobre o mesmo param.

// 1) closure MUTA o parâmetro capturado
function makeParamCounter(n: number) {
  return () => {
    n += 1;
    return n;
  };
}
const pc = makeParamCounter(10);
console.log("mutate_param=" + pc() + "," + pc() + "," + pc());

// 2) corpo muta o param depois de criar a closure; closure lê o valor atual
function bodyMutates(p: string) {
  const read = () => p;
  const first = read();
  p = p.toUpperCase();
  return first + ":" + read();
}
console.log("body_mutates=" + bodyMutates("ab"));

// 3) getter + setter sobre o MESMO param
function paramPair(seed: number) {
  return {
    get: () => seed,
    set: (x: number) => {
      seed = x;
    },
    bump: () => {
      seed += 5;
      return seed;
    },
  };
}
const pp = paramPair(1);
console.log("pair_init=" + pp.get());
pp.set(20);
console.log("pair_after_set=" + pp.get());
console.log("pair_bump=" + pp.bump() + " get=" + pp.get());

// 4) parâmetro com default: o binding é o mesmo, defaultado ou não
function withDefault(a: number = 7) {
  return () => {
    a *= 2;
    return a;
  };
}
const wd1 = withDefault();
const wd2 = withDefault(3);
console.log("default=" + wd1() + "," + wd1() + " explicit=" + wd2() + "," + wd2());

// 5) rest param capturado e mutado (push no array do rest)
function restCapture(...items: number[]) {
  return {
    add: (x: number) => {
      items.push(x);
      return items.length;
    },
    sum: () => items.reduce((s, i) => s + i, 0),
    rebind: () => {
      items = [];
      return items.length;
    },
  };
}
const rc = restCapture(1, 2);
console.log("rest_add=" + rc.add(3) + " sum=" + rc.sum());
console.log("rest_rebind=" + rc.rebind() + " sum=" + rc.sum());

// 6) param destructurado: cada nome é seu próprio binding mutável
function destructured({ a, b }: { a: number; b: number }) {
  const read = () => a + "/" + b;
  a = a + 100;
  return { read, bumpB: () => { b += 1; return b; } };
}
const de = destructured({ a: 1, b: 2 });
console.log("destructured=" + de.read());
console.log("destructured_bumpB=" + de.bumpB() + " read=" + de.read());

// 7) param usado como acumulador por várias closures irmãs
function sharedParam(total: number) {
  const addA = (x: number) => {
    total += x;
  };
  const addB = (x: number) => {
    total += x * 10;
  };
  const get = () => total;
  return { addA, addB, get };
}
const sp = sharedParam(0);
sp.addA(1);
sp.addB(2);
sp.addA(3);
console.log("shared_param=" + sp.get());

// 8) arg passado por valor: mutar o param NÃO afeta a variável do chamador
let caller = 5;
function mutateArg(p: number) {
  p = 999;
  return () => p;
}
const ma = mutateArg(caller);
console.log("arg_by_value caller=" + caller + " inner=" + ma());
