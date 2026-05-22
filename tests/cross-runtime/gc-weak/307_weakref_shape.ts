// Cross-runtime: WeakRef API shape.
const obj = { a: 1 };
const wr = new WeakRef(obj);
console.log("type=" + typeof WeakRef);
console.log("deref=" + typeof wr.deref());
