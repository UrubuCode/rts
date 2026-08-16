// Cross-runtime: a Promise SUBCLASS decides what `then`, `catch` and `finally`
// hand back, and `Symbol.species` overrides that decision. Focus: which
// constructor is invoked, and how many times.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const built: string[] = [];

class Tracked extends Promise {
  constructor(executor: any) {
    super(executor);
    built.push("Tracked");
  }
}

// 1) then/catch/finally on a subclass instance answer the subclass
const t = Tracked.resolve("v") as any;
log("resolveIsTracked=" + (t instanceof Tracked));
const t2 = t.then(function (v: any) { return v; });
log("thenIsTracked=" + (t2 instanceof Tracked));
log("thenIsPromise=" + (t2 instanceof Promise));
log("catchIsTracked=" + (t.catch(function () { return 0; }) instanceof Tracked));
log("finallyIsTracked=" + (t.finally(function () { return 0; }) instanceof Tracked));
log("constructorName=" + t2.constructor.name);

// 2) species pointing back at Promise makes the derived promises plain
class Plain extends Promise {
  static get [Symbol.species]() { return Promise; }
}
const p = Plain.resolve("w") as any;
log("plainResolveIsPlain=" + (p instanceof Plain));
const p2 = p.then(function (v: any) { return v; });
log("plainThenIsPlain=" + (p2 instanceof Plain));
log("plainThenIsPromise=" + (p2 instanceof Promise));
log("plainThenCtor=" + p2.constructor.name);
log("plainCatchIsPlain=" + (p.catch(function () { return 0; }) instanceof Plain));

// 3) the base class's own species is itself
log("promiseSpecies=" + (Promise[Symbol.species] === Promise));
log("trackedSpeciesInherited=" + ((Tracked as any)[Symbol.species] === Tracked));

// 4) combinators build with the RECEIVER, not with Promise
built.length = 0;
const all = Tracked.all([1, 2]) as any;
log("allIsTracked=" + (all instanceof Tracked));
log("allBuilt=" + built.length);
built.length = 0;
const race = Tracked.race([Promise.resolve(1)]) as any;
log("raceIsTracked=" + (race instanceof Tracked));
built.length = 0;
const settled = Tracked.allSettled([1]) as any;
log("allSettledIsTracked=" + (settled instanceof Tracked));

// 5) a subclass instance still flows through a plain chain as a value
const flowed = Promise.resolve(Tracked.resolve("flow") as any);
log("flowedIsTracked=" + (flowed instanceof Tracked));

// 6) the prototype chain, with no surprises
log("protoChain=" + (Object.getPrototypeOf(Tracked.prototype) === Promise.prototype));
log("staticChain=" + (Object.getPrototypeOf(Tracked) === Promise));

// 7) drain and finish
Promise.all([t2, p2, all, race, settled, flowed]).then(function (vs: any[]) {
  log("values=" + vs.map(function (x: any) { return Array.isArray(x) ? "[" + x.length + "]" : String(x && x.status ? x.status : x); }).join("|"));
  console.log("end");
});
