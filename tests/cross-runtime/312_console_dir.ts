// Cross-runtime: console.dir API shape.
const calls: string[] = [];
const d = console.dir;
(console as any).dir = (arg: any, opts: any) => { calls.push(typeof arg + ":" + String(opts?.depth)); };
console.dir({ a: { b: 1 } }, { depth: 1 });
(console as any).dir = d;
console.log("calls=" + calls.join("|"));
