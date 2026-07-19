// closures capturando var → exercita __RTS_FN_NS_GC_CLOSURE/ENV_* (agora no engine).
function makeAdder(n: number): (x: number) => number {
  return (x: number) => x + n;          // captura n → env + closure alloc
}
function makeCounter(): () => number {
  let c = 0;
  return () => { c = c + 1; return c; };  // captura mutável
}
const add10 = makeAdder(10);
const add100 = makeAdder(100);
console.log("add10(5)=" + add10(5));
console.log("add100(5)=" + add100(5));
const ctr = makeCounter();
console.log("ctr=" + ctr() + "," + ctr() + "," + ctr());
console.log("CLOSURE_OK");
