// Cross-runtime: ownKeys may not omit a non-configurable target property.
const target: any = {};
Object.defineProperty(target, "fixed", { value: 1, configurable: false });
const proxy = new Proxy(target, { ownKeys() { return []; } });
let keysError = false;
let namesError = false;
try { Reflect.ownKeys(proxy); } catch (e) { keysError = e instanceof TypeError; }
try { Object.getOwnPropertyNames(proxy); } catch (e) { namesError = e instanceof TypeError; }
console.log(keysError, namesError);
console.log(target.fixed);

