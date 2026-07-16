// Cross-runtime: UMA coisa — várias funções retornadas de uma IIFE compartilham
// o MESMO binding privado. Distinto de claude-shared-closure-var (lá são 2 fns e
// um get/set simples); aqui o foco é o "module pattern" completo: get/inc/reset/
// add, múltiplas instâncias independentes, aliasing, e estado privado inacessível.

const counter = (function () {
  let count = 0;
  let history: number[] = [];
  function bump(by: number): number {
    count += by;
    history.push(count);
    return count;
  }
  return {
    get: (): number => count,
    inc: (): number => bump(1),
    add: (n: number): number => bump(n),
    reset: (): number => {
      count = 0;
      history = [];
      return count;
    },
    size: (): number => history.length,
    trail: (): string => history.join("|"),
  };
})();

console.log("init=" + counter.get());
console.log("inc1=" + counter.inc());
console.log("inc2=" + counter.inc());
console.log("get_sees_inc=" + counter.get());
console.log("add10=" + counter.add(10));
console.log("get_after_add=" + counter.get());
console.log("trail=" + counter.trail());
console.log("size=" + counter.size());
console.log("reset=" + counter.reset());
console.log("get_after_reset=" + counter.get());
console.log("size_after_reset=" + counter.size());
console.log("inc_after_reset=" + counter.inc());

// duas instâncias da MESMA factory têm bindings independentes
function makeModule(start: number) {
  let value = start;
  const get = () => value;
  const inc = () => {
    value += 1;
    return value;
  };
  const set = (v: number) => {
    value = v;
    return value;
  };
  return { get, inc, set };
}

const m1 = makeModule(0);
const m2 = makeModule(100);
m1.inc();
m1.inc();
m2.inc();
console.log("m1=" + m1.get() + " m2=" + m2.get());
m1.set(50);
console.log("after_set m1=" + m1.get() + " m2=" + m2.get());

// aliasing: extrair o método perde `this` mas NÃO o binding capturado
const looseInc = m2.inc;
const looseGet = m2.get;
looseInc();
console.log("detached_alias=" + looseGet() + " via_obj=" + m2.get());

// closures criadas em IIFEs irmãs não se enxergam
const isolated = [0, 1].map((seed) =>
  (function () {
    let v = seed;
    return {
      bump: () => {
        v += 10;
        return v;
      },
      peek: () => v,
    };
  })()
);
isolated[0].bump();
console.log("isolated=" + isolated[0].peek() + "," + isolated[1].peek());
