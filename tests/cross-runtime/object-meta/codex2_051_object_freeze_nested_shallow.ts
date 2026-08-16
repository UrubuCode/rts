// Cross-runtime: freezing is shallow and locks top-level data properties.
export {};
const o: any = { inner: { n: 1 }, top: 2 };
Object.freeze(o);
let writeError = false;
try { o.top = 9; } catch (e) { writeError = e instanceof TypeError; }
o.inner.n = 7;
console.log(o.top, o.inner.n);
console.log(Object.isFrozen(o), Object.isFrozen(o.inner), writeError);
