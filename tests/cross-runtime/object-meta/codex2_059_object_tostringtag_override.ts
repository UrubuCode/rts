// Cross-runtime: Symbol.toStringTag customizes the generic object tag.
const o = { [Symbol.toStringTag]: "Widget" };
console.log(Object.prototype.toString.call(o));
delete (o as any)[Symbol.toStringTag];
console.log(Object.prototype.toString.call(o));

