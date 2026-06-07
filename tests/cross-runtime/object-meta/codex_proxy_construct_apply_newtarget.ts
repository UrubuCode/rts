// Cross-runtime: Proxy apply/construct traps and newTarget behavior.
function F(this: any, x: number) {
  this.x = x;
  return x + 1;
}

const P: any = new Proxy(F, {
  apply(target, thisArg, args) {
    return Reflect.apply(target, { x: 0 }, args) + 10;
  },
  construct(target, args, newTarget) {
    const obj = Reflect.construct(target, args, newTarget);
    obj.tag = newTarget === P ? "proxy" : "other";
    return obj;
  }
});

console.log(P(2));
const inst = new P(5);
console.log(inst.x + ":" + inst.tag + ":" + (inst instanceof F) + ":" + (inst instanceof P));
