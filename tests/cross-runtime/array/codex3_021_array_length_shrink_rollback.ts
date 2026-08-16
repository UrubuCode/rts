// Cross-runtime: shrinking length rolls back when a non-configurable index blocks deletion.
export {};
const a: any[] = [0, 1, 2, 3, 4];
Object.defineProperty(a, "2", { configurable: false, writable: true, value: 2 });
let threw = false;
try { a.length = 1; } catch (e) { threw = e instanceof TypeError; }
console.log(threw, a.length, Object.keys(a).join(","));
console.log(a[2], a[3], a[4]);

