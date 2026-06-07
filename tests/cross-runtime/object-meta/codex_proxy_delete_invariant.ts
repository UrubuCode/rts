// Cross-runtime: deleteProperty trap cannot report success for non-configurable own property.
const target: any = {};
Object.defineProperty(target, "x", { value: 1, configurable: false });
const proxy = new Proxy(target, {
  deleteProperty() {
    return true;
  }
});

try {
  delete (proxy as any).x;
} catch (e: any) {
  console.log(e.constructor.name);
}
console.log(target.x);
