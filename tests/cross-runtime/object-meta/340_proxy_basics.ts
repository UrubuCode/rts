// Cross-runtime: Proxy with get/set/has/deleteProperty traps.
const log: string[] = [];

const target = { a: 1, b: 2 };
const proxy = new Proxy(target, {
  get(t, key) {
    log.push("get:" + String(key));
    return (t as any)[key];
  },
  set(t, key, val) {
    log.push("set:" + String(key) + "=" + val);
    (t as any)[key] = val;
    return true;
  },
  has(t, key) {
    log.push("has:" + String(key));
    return key in t;
  },
  deleteProperty(t, key) {
    log.push("del:" + String(key));
    delete (t as any)[key];
    return true;
  }
});

console.log(proxy.a);
proxy.b = 99;
console.log("b" in proxy);
delete (proxy as any).b;
console.log(log.join("|"));

// Proxy as validator
function createPositive(obj: Record<string, number>) {
  return new Proxy(obj, {
    set(t, key, val) {
      if (typeof val !== "number" || val < 0) throw new RangeError("must be positive");
      t[key as string] = val;
      return true;
    }
  });
}
const pos = createPositive({});
pos.x = 5;
console.log(pos.x);
try { pos.y = -1; } catch (e: any) { console.log(e.message); }
