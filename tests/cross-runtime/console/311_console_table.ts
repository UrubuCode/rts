// Cross-runtime: console.table API shape.
const calls: string[] = [];
const t = console.table;
(console as any).table = (arg: any) => { calls.push(Array.isArray(arg) ? String(arg.length) : typeof arg); };
console.table([{ a: 1 }, { a: 2 }]);
(console as any).table = t;
console.log("calls=" + calls.join("|"));
