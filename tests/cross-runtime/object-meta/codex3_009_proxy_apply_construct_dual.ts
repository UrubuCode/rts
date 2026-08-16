// Cross-runtime: a callable constructor Proxy can distinguish apply from construct.
function Sum(this: any, a: number, b: number) {
  if (new.target) this.value = a + b;
  return a + b;
}
const seen: string[] = [];
const proxy: any = new Proxy(Sum, {
  apply(target, thisArg, args) { seen.push("apply:" + args.join(",")); return Reflect.apply(target, thisArg, args) * 2; },
  construct(target, args, newTarget) { seen.push("construct:" + args.join(",") + ":" + (newTarget === proxy)); return Reflect.construct(target, args, newTarget); },
});
console.log(proxy(2, 3));
const value = new proxy(4, 5);
console.log(value.value, value instanceof proxy, value instanceof Sum);
console.log(seen.join("|"));

