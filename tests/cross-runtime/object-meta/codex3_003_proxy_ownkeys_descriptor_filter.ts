// Cross-runtime: ownKeys order is filtered through getOwnPropertyDescriptor enumerability.
const target: any = { a: 1, b: 2, c: 3 };
const proxy = new Proxy(target, {
  ownKeys() { return ["c", "hidden", "a", "b"]; },
  getOwnPropertyDescriptor(t, key) {
    if (key === "hidden") return { value: 9, enumerable: false, configurable: true };
    if (key === "b") return { value: 2, enumerable: false, configurable: true };
    return Reflect.getOwnPropertyDescriptor(t, key);
  },
});
console.log(Reflect.ownKeys(proxy).join(","));
console.log(Object.keys(proxy).join(","));
console.log(JSON.stringify({ ...proxy }));

