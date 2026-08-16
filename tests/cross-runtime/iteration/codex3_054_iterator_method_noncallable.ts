// Cross-runtime: a present non-callable Symbol.iterator produces TypeError at each consumer.
const value: any = { [Symbol.iterator]: 3 };
const checks: boolean[] = [];
for (const op of [() => [...value], () => Array.from(value), () => { for (const x of value) void x; }]) {
  try { op(); checks.push(false); } catch (e) { checks.push(e instanceof TypeError); }
}
console.log(checks.join(","));

