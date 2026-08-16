// Cross-runtime: a Proxy get trap forwarding with Reflect preserves accessor receiver.
const target = {
  value: 4,
  get doubled() { return this.value * 2; },
};
const seen: string[] = [];
const proxy = new Proxy(target, {
  get(t, key, receiver) {
    seen.push(String(key) + ":" + (receiver === proxy));
    return Reflect.get(t, key, receiver);
  },
});
const child = Object.create(proxy);
child.value = 7;
console.log(child.doubled);
console.log(seen.join("|"));

