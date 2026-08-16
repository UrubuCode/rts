// Cross-runtime: throw() and return() on a generator that has NOT started, and
// on one that has already finished. The body never runs in either case -- no
// `try` is entered, so no `finally` fires -- and the generator ends completed.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const trace: string[] = [];

function* guarded(tag: string) {
  trace.push(tag + ":body-entered");
  try {
    trace.push(tag + ":try");
    yield tag + "-1";
    yield tag + "-2";
  } catch (e: any) {
    trace.push(tag + ":caught:" + String(e));
    yield tag + "-recovered";
  } finally {
    trace.push(tag + ":finally");
  }
  trace.push(tag + ":after");
}

// 1) throw() before the first next(): the body is never entered, so the catch
//    does NOT see it -- the error comes straight back out
trace.length = 0;
const a = guarded("A");
log("throwBeforeStart=" + (function () {
  try { a.throw("E"); return "no-throw"; } catch (e: any) { return "threw:" + String(e); }
})());
log("traceA=" + JSON.stringify(trace.join(",")));
log("aAfter=" + JSON.stringify(a.next()));

// 2) return() before the first next(): done immediately with that value, body
//    never entered
trace.length = 0;
const b = guarded("B");
log("returnBeforeStart=" + JSON.stringify(b.return("RB")));
log("traceB=" + JSON.stringify(trace.join(",")));
log("bAfter=" + JSON.stringify(b.next()));

// 3) return() with no argument before start
const c = guarded("C");
log("returnNoArg=" + JSON.stringify(c.return(undefined as any)));

// 4) for comparison: throw() AFTER the first next() is caught by the body
trace.length = 0;
const d = guarded("D");
log("dFirst=" + d.next().value);
log("dThrown=" + JSON.stringify(d.throw("E2")));
log("traceD=" + trace.join(","));
log("dRest=" + JSON.stringify(d.next()));
log("traceDFull=" + trace.join(","));

// 5) throw() on a COMPLETED generator throws back out, and the body stays put
trace.length = 0;
const e = guarded("E");
e.next(); e.next(); e.next(); e.next();
const completedTrace = trace.join(",");
log("completedTrace=" + completedTrace);
trace.length = 0;
log("throwOnCompleted=" + (function () {
  try { e.throw("E3"); return "no-throw"; } catch (err: any) { return "threw:" + String(err); }
})());
log("traceAfterCompletedThrow=" + JSON.stringify(trace.join(",")));

// 6) return() on a COMPLETED generator answers done with the value, quietly
log("returnOnCompleted=" + JSON.stringify(e.return("RE")));
log("nextOnCompleted=" + JSON.stringify(e.next()));

// 7) a generator whose body has no try at all: return() before start still
//    never runs it
trace.length = 0;
function* plain() { trace.push("plain:body"); yield 1; }
const f = plain();
log("plainReturn=" + JSON.stringify(f.return("RF")) + " trace=" + JSON.stringify(trace.join(",")));

// 8) throw() before start on a generator with a top-level try/finally: still no
//    finally, because the frame was never entered
trace.length = 0;
function* fin() { try { yield 1; } finally { trace.push("fin:finally"); } }
const g = fin();
log("finThrowBeforeStart=" + (function () {
  try { g.throw("E4"); return "no-throw"; } catch (err: any) { return "threw:" + String(err); }
})());
log("finTrace=" + JSON.stringify(trace.join(",")));

// 9) but once started, return() DOES run the finally
trace.length = 0;
const h = fin();
h.next();
log("finReturnAfterStart=" + JSON.stringify(h.return("RH")) + " trace=" + trace.join(","));

// 10) throw() with no argument at all uses undefined as the exception
const i = guarded("I");
i.next();
log("throwUndefined=" + JSON.stringify(i.throw(undefined as any)));

// 11) the `done` generator is still an iterable, and for-of over it yields
//     nothing
const j = plain();
j.return("done");
const collected: string[] = [];
for (const v of j as any) collected.push(String(v));
log("forOfOnDone=" + JSON.stringify(collected.join(",")));

console.log("end");
