// Cross-runtime: deleteProperty cannot report deletion of a non-configurable property.
const target: any = {};
Object.defineProperty(target, "fixed", { value: 5, configurable: false });
const proxy = new Proxy(target, { deleteProperty() { return true; } });
let reflectError = false;
try { Reflect.deleteProperty(proxy, "fixed"); } catch (e) { reflectError = e instanceof TypeError; }
console.log(reflectError, Object.hasOwn(target, "fixed"));

