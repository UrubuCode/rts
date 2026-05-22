// Cross-runtime compatibility: Proxy traps and Reflect.
const target = { a: 1, b: 2 };
const seen: string[] = [];

const proxy = new Proxy(target, {
  get(obj, prop, recv) {
    seen.push("get:" + String(prop));
    return Reflect.get(obj, prop, recv);
  },
  set(obj, prop, value, recv) {
    seen.push("set:" + String(prop) + "=" + String(value));
    return Reflect.set(obj, prop, value, recv);
  },
  has(obj, prop) {
    seen.push("has:" + String(prop));
    return Reflect.has(obj, prop);
  },
});

console.log("ga=" + proxy.a);
proxy.b = 9;
console.log("hb=" + ("b" in proxy));
console.log("vals=" + proxy.a + ":" + proxy.b);
console.log("seen=" + seen.join(","));
