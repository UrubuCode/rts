// Cross-runtime: strict arguments.callee and caller accessors throw TypeError.
export {};
function inspect() {
  const results: boolean[] = [];
  try { void (arguments as any).callee; } catch (e) { results.push(e instanceof TypeError); }
  try { void (inspect as any).caller; } catch (e) { results.push(e instanceof TypeError); }
  return results;
}
console.log(inspect().join(","));

