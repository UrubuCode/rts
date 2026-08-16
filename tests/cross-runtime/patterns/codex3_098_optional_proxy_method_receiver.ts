// Cross-runtime: optional method calls through Proxy preserve receiver and skip missing arguments.
const seen: string[] = [];
const target = {
  value: 5,
  run(n: number) { return this.value + n; },
};
const proxy: any = new Proxy(target, {
  get(t, key, receiver) {
    seen.push("get:" + String(key));
    return Reflect.get(t, key, receiver);
  },
});
let args = 0;
const arg = () => { args++; return 3; };
console.log(proxy.run?.(arg()));
console.log(proxy.missing?.(arg()));
console.log(args, seen.join("|"));

