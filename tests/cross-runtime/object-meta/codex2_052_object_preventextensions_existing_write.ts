// Cross-runtime: preventExtensions blocks new keys but preserves existing writes.
export {};
const o: any = { x: 1 };
Object.preventExtensions(o);
o.x = 4;
let addError = false;
try { o.y = 5; } catch (e) { addError = e instanceof TypeError; }
console.log(o.x, o.y, Object.keys(o).join(","));
console.log(Object.isExtensible(o), Object.isSealed(o), addError);
