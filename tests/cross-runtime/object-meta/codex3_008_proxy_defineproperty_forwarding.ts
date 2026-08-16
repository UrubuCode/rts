// Cross-runtime: defineProperty trap observes normalized descriptors and may forward them.
const target: any = {};
const seen: string[] = [];
const proxy = new Proxy(target, {
  defineProperty(t, key, descriptor) {
    seen.push(String(key) + ":" + String(descriptor.value) + ":" + descriptor.enumerable);
    return Reflect.defineProperty(t, key, descriptor);
  },
});
Object.defineProperty(proxy, "x", { value: 4, enumerable: true, configurable: true });
Reflect.defineProperty(proxy, "y", { value: 5, writable: true });
console.log(seen.join("|"));
console.log(JSON.stringify(Object.getOwnPropertyDescriptors(target)));

