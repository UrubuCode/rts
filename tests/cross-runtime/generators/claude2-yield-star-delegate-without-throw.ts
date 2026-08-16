// Cross-runtime: `yield*` over a delegate that lacks `throw`. The outer
// generator CLOSES the delegate (calling its `return` if it has one) and then
// raises a TypeError at the delegation site -- it does not forward the value.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const trace: string[] = [];

// A delegate with next and return but NO throw
function makeNoThrow(tag: string) {
  let i = 0;
  return {
    [Symbol.iterator]: function () { return this; },
    next: function () { trace.push(tag + ".next"); return i < 3 ? { done: false, value: tag + (i++) } : { done: true, value: undefined }; },
    return: function (v: any) { trace.push(tag + ".return(" + String(v) + ")"); return { done: true, value: "closed-" + tag }; }
  };
}

// 1) throw() into a generator suspended in `yield*` over a delegate with no
//    throw: the delegate is closed, and a TypeError comes out
trace.length = 0;
function* outerA() {
  try {
    yield* makeNoThrow("A") as any;
  } finally {
    trace.push("outerA-finally");
  }
}
const ga = outerA();
log("firstValue=" + ga.next().value);
log("throwResult=" + (function () {
  try { ga.throw(new Error("x")); return "no-throw"; } catch (e: any) { return e.constructor.name; }
})());
log("trace=" + trace.join(","));
log("generatorDone=" + JSON.stringify(ga.next()));

// 2) a delegate with NEITHER throw nor return: still a TypeError, nothing to
//    close
trace.length = 0;
function makeBare(tag: string) {
  let i = 0;
  return {
    [Symbol.iterator]: function () { return this; },
    next: function () { trace.push(tag + ".next"); return i < 3 ? { done: false, value: tag + (i++) } : { done: true, value: undefined }; }
  };
}
function* outerB() { yield* makeBare("B") as any; }
const gb = outerB();
gb.next();
log("bareThrow=" + (function () {
  try { gb.throw(new Error("x")); return "no-throw"; } catch (e: any) { return e.constructor.name; }
})());
log("bareTrace=" + trace.join(","));

// 3) return() into a `yield*` over a delegate WITHOUT return: the outer
//    generator simply finishes with the value, no error
trace.length = 0;
function* outerC() {
  try { yield* makeBare("C") as any; } finally { trace.push("outerC-finally"); }
}
const gc = outerC();
gc.next();
log("returnOnBare=" + JSON.stringify(gc.return("R")));
log("returnTrace=" + trace.join(","));

// 4) return() into a `yield*` over a delegate WITH return: the delegate's
//    return is called with the value, and its answer does NOT replace it
trace.length = 0;
function* outerD() { yield* makeNoThrow("D") as any; }
const gd = outerD();
gd.next();
log("returnOnClosable=" + JSON.stringify(gd.return("RD")));
log("returnClosableTrace=" + trace.join(","));

// 5) a `return` method that answers a NON-object is a TypeError on the
//    return path
trace.length = 0;
function makeBadReturn() {
  let i = 0;
  return {
    [Symbol.iterator]: function () { return this; },
    next: function () { return i < 3 ? { done: false, value: i++ } : { done: true, value: undefined }; },
    return: function () { trace.push("badReturn"); return 42 as any; }
  };
}
function* outerE() { yield* makeBadReturn() as any; }
const ge = outerE();
ge.next();
log("badReturn=" + (function () {
  try { ge.return("X"); return "no-throw"; } catch (e: any) { return e.constructor.name; }
})());
log("badReturnTrace=" + trace.join(","));

// 6) a generator delegate HAS throw, so the same throw is forwarded and can be
//    caught inside it -- the contrast case
trace.length = 0;
function* innerF() {
  try { yield "f0"; yield "f1"; }
  catch (e: any) { trace.push("innerF-caught:" + String(e)); yield "recovered"; }
}
function* outerF() { const r = yield* innerF(); trace.push("delegationValue=" + String(r)); yield "after"; }
const gf = outerF();
log("fFirst=" + gf.next().value);
log("fThrown=" + gf.throw("BOOM").value);
log("fNext=" + JSON.stringify(gf.next()));
log("fTrace=" + trace.join(","));

// 7) the TypeError case leaves the OUTER generator completed, so a later
//    next() answers done
log("afterTypeErrorNext=" + JSON.stringify(gb.next()));
log("afterTypeErrorReturn=" + JSON.stringify(gb.return("Z")));

console.log("end");
