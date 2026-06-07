// Cross-runtime: Proxy exposes a virtual configurable property through descriptors.
const proxy = new Proxy({}, {
  ownKeys() {
    return ["virtual"];
  },
  getOwnPropertyDescriptor(_t, k) {
    if (k === "virtual") return { value: 42, enumerable: true, configurable: true };
    return undefined;
  },
  get(_t, k) {
    return k === "virtual" ? 42 : undefined;
  }
});

console.log(Object.keys(proxy).join(","));
console.log((proxy as any).virtual);
console.log(JSON.stringify(proxy));
