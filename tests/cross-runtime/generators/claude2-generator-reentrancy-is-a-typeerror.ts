// Cross-runtime: a generator that is RUNNING refuses to be driven again.
// next(), throw() and return() called from inside its own body all raise a
// TypeError, and the generator survives it -- the outer next() still finishes.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

// 1) next() on itself, from inside the body
let selfNext = "unset";
function* reNext(): Generator<any, any, any> {
  try { (g1 as any).next(); selfNext = "no-throw"; }
  catch (e: any) { selfNext = e.constructor.name; }
  yield "after";
  return "done";
}
const g1 = reNext();
log("firstValue=" + g1.next().value);
log("selfNext=" + selfNext);
log("stillAlive=" + JSON.stringify(g1.next()));

// 2) throw() on itself
let selfThrow = "unset";
function* reThrow(): Generator<any, any, any> {
  try { (g2 as any).throw(new Error("x")); selfThrow = "no-throw"; }
  catch (e: any) { selfThrow = e.constructor.name; }
  yield "after";
}
const g2 = reThrow();
g2.next();
log("selfThrow=" + selfThrow);

// 3) return() on itself
let selfReturn = "unset";
function* reReturn(): Generator<any, any, any> {
  try { (g3 as any).return("r"); selfReturn = "no-throw"; }
  catch (e: any) { selfReturn = e.constructor.name; }
  yield "after";
}
const g3 = reReturn();
g3.next();
log("selfReturn=" + selfReturn);

// 4) re-entry through a CALLBACK the body invokes is the same situation
let viaCallback = "unset";
function* reCallback(): Generator<any, any, any> {
  const poke = function () { (g4 as any).next(); };
  try { poke(); viaCallback = "no-throw"; } catch (e: any) { viaCallback = e.constructor.name; }
  yield "cb";
}
const g4 = reCallback();
g4.next();
log("viaCallback=" + viaCallback);

// 5) re-entry from inside a `finally` that a return() is running
let inFinally = "unset";
function* reFinally(): Generator<any, any, any> {
  try { yield "a"; }
  finally {
    try { (g5 as any).next(); inFinally = "no-throw"; } catch (e: any) { inFinally = e.constructor.name; }
  }
}
const g5 = reFinally();
g5.next();
log("returnResult=" + JSON.stringify(g5.return("R")));
log("inFinally=" + inFinally);

// 6) a generator SUSPENDED at a yield is not running: driving it from another
//    generator's body is fine
function* driver(target: Generator<any, any, any>) {
  yield "d:" + target.next().value;
  yield "d:" + target.next().value;
}
function* numbers() { yield "n1"; yield "n2"; yield "n3"; }
const target = numbers();
const d = driver(target);
log("driven=" + d.next().value + "," + d.next().value);
log("targetLeftover=" + JSON.stringify(target.next()));

// 7) two generators driving each other, alternating, never overlap
const pings: string[] = [];
function* ping(): Generator<any, any, any> { for (let i = 0; i < 3; i++) { pings.push("ping" + i); yield i; } }
function* pong(p: Generator<any, any, any>): Generator<any, any, any> {
  let r = p.next();
  while (!r.done) { pings.push("pong:" + r.value); yield r.value; r = p.next(); }
}
const po = pong(ping());
while (!po.next().done) { }
log("alternating=" + pings.join(","));

// 8) a COMPLETED generator is not running either -- next() is quiet, not an error
function* short() { yield 1; }
const s = short();
s.next(); s.next();
log("completedNext=" + JSON.stringify(s.next()) + " completedReturn=" + JSON.stringify(s.return("q")));

// 9) the TypeError from re-entry is catchable OUTSIDE too: a generator driven
//    while another next() is on the stack, from a nested call
let outerReentry = "unset";
function* nested(): Generator<any, any, any> { yield helper(); }
function helper(): string {
  try { (g9 as any).next(); return "no-throw"; }
  catch (e: any) { outerReentry = e.constructor.name; return "caught"; }
}
const g9 = nested();
log("nestedValue=" + g9.next().value + " outerReentry=" + outerReentry);

// 10) the refused call leaves the generator's position untouched: g1 still
//     yields the rest of its body in order
log("g1Rest=" + JSON.stringify(g1.next()));

// 11) spread over a generator that pokes itself surfaces the same TypeError
let inSpread = "unset";
function* pokes(): Generator<any, any, any> {
  yield 1;
  try { (gs as any).next(); inSpread = "no-throw"; } catch (e: any) { inSpread = e.constructor.name; }
  yield 2;
}
const gs = pokes();
log("spread=" + [...(gs as any)].join(",") + " inSpread=" + inSpread);

// 12) a for-of that drives the SAME generator from inside its own body sees the
//     refusal too
let inForOf = "unset";
function* selfLoop(): Generator<any, any, any> {
  yield "s1";
  try { for (const v of gl as any) { inForOf = "iterated:" + v; break; } }
  catch (e: any) { inForOf = e.constructor.name; }
  yield "s2";
}
const gl = selfLoop();
gl.next();
gl.next();
log("selfForOf=" + inForOf);

// 13) a generator delegating with yield* is running while the delegate runs, so
//     poking the OUTER one from the inner body is refused
let inDelegate = "unset";
function* innerPoke(): Generator<any, any, any> {
  try { (go as any).next(); inDelegate = "no-throw"; } catch (e: any) { inDelegate = e.constructor.name; }
  yield "inner";
}
function* outerDelegate(): Generator<any, any, any> { yield* innerPoke(); }
const go = outerDelegate();
log("delegateValue=" + go.next().value + " inDelegate=" + inDelegate);

console.log("end");
