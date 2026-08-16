// Cross-runtime: has cannot hide a non-configurable own property.
const target: any = {};
Object.defineProperty(target, "secret", { value: 7, configurable: false });
const proxy = new Proxy(target, { has() { return false; } });
let threw = false;
try { void ("secret" in proxy); } catch (e) { threw = e instanceof TypeError; }
console.log(threw, Reflect.has(target, "secret"));

