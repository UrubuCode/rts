// Cross-runtime compatibility: Proxy ownKeys/get/delete/defineProperty.
const log: string[] = [];
const proxy = new Proxy(
  { a: 1, b: 2 },
  {
    ownKeys(target) {
      log.push("ownKeys");
      return Reflect.ownKeys(target);
    },
    getOwnPropertyDescriptor(target, prop) {
      log.push("desc:" + String(prop));
      return Reflect.getOwnPropertyDescriptor(target, prop);
    },
    deleteProperty(target, prop) {
      log.push("del:" + String(prop));
      return Reflect.deleteProperty(target, prop);
    },
    defineProperty(target, prop, desc) {
      log.push("def:" + String(prop) + "=" + String((desc as PropertyDescriptor).value));
      return Reflect.defineProperty(target, prop, desc);
    },
  },
);

Object.keys(proxy);
Object.defineProperty(proxy, "c", { value: 3, enumerable: true, configurable: true });
delete proxy.b;
console.log("keys=" + Object.keys(proxy).join(","));
console.log("vals=" + JSON.stringify(proxy));
console.log("log=" + log.join("|"));
