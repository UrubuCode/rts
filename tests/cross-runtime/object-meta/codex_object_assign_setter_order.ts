// Cross-runtime: Object.assign invokes source getters and target setters in order.
const log: string[] = [];
const target: any = {
  set a(v: number) { log.push("set-a:" + v); },
};
const source: any = {
  get a() { log.push("get-a"); return 1; },
  get b() { log.push("get-b"); return 2; },
};

const out = Object.assign(target, source, { a: 3 });
console.log(out === target);
console.log(out.b);
console.log(log.join("|"));
