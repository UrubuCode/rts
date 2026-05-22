// Cross-runtime: Error cause in built-in errors.
const root = new Error("root");
const e1 = new Error("msg", { cause: root as any });
const e2 = new TypeError("bad", { cause: e1 as any });
console.log("e1=" + e1.message + ":" + (e1 as any).cause.message);
console.log("e2=" + e2.name + ":" + (e2 as any).cause.message);
