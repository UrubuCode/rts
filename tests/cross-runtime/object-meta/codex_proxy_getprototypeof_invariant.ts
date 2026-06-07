// Cross-runtime: getPrototypeOf trap must report actual prototype for non-extensible target.
const proto = { p: 1 };
const target = Object.create(proto);
Object.preventExtensions(target);
const proxy = new Proxy(target, {
  getPrototypeOf() {
    return {};
  }
});

try {
  Object.getPrototypeOf(proxy);
} catch (e: any) {
  console.log(e.constructor.name);
}
console.log(Object.getPrototypeOf(target) === proto);
