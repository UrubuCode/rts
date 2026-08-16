// Cross-runtime: get cannot lie about a frozen data property's value.
const target: any = {};
Object.defineProperty(target, "fixed", { value: 3, writable: false, configurable: false });
const proxy = new Proxy(target, { get() { return 99; } });
let threw = false;
try { void proxy.fixed; } catch (e) { threw = e instanceof TypeError; }
console.log(threw, target.fixed);

