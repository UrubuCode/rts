// Cross-runtime: Proxy cannot hide non-configurable properties.
const target: any = {};
Object.defineProperty(target, "fixed", { value: 1, configurable: false, enumerable: true });
const proxy = new Proxy(target, {
  ownKeys() {
    return [];
  }
});

try {
  Reflect.ownKeys(proxy);
} catch (e: any) {
  console.log(e.constructor.name);
}
console.log(target.fixed);
