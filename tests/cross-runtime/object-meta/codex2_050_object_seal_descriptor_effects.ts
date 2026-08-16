// Cross-runtime: sealing prevents extension and makes own properties non-configurable.
export {};
const o: any = { x: 1 };
Object.seal(o);
o.x = 2;
let addError = false;
let deleteError = false;
try { o.y = 3; } catch (e) { addError = e instanceof TypeError; }
try { delete o.x; } catch (e) { deleteError = e instanceof TypeError; }
const d = Object.getOwnPropertyDescriptor(o, "x")!;
console.log(o.x, o.y, d.writable, d.configurable);
console.log(Object.isSealed(o), Object.isExtensible(o), addError, deleteError);
