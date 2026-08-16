// Cross-runtime: every essential operation on a revoked Proxy throws.
const pair = Proxy.revocable({ x: 1 }, {});
console.log(pair.proxy.x);
pair.revoke();
const results: boolean[] = [];
for (const op of [
  () => pair.proxy.x,
  () => Reflect.ownKeys(pair.proxy),
  () => Object.getPrototypeOf(pair.proxy),
]) {
  try { op(); results.push(false); } catch (e) { results.push(e instanceof TypeError); }
}
console.log(results.join(","));

