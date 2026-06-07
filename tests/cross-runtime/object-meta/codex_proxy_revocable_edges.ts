// Cross-runtime: Proxy.revocable throws after revoke.
const target = { a: 1 };
const { proxy, revoke } = Proxy.revocable(target, {
  get(t, k, r) {
    return Reflect.get(t, k, r);
  }
});

console.log((proxy as any).a);
revoke();
for (const op of ["get", "keys"]) {
  try {
    if (op === "get") console.log((proxy as any).a);
    else Object.keys(proxy);
  } catch (e: any) {
    console.log(op + ":" + e.constructor.name);
  }
}
