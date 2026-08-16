// Cross-runtime: AbortSignal.any composes signals -- it aborts with the reason
// of the FIRST source to abort, is already aborted if any source is, and an
// empty list never aborts. No timers are used, so nothing here can race.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const seen: string[] = [];

// 0) the surface exists in both runtimes
log("hasAny=" + (typeof AbortSignal.any));
log("anyLength=" + AbortSignal.any.length);

// 1) an empty list gives a signal that is not aborted and stays that way
const empty = AbortSignal.any([]);
log("emptyAborted=" + empty.aborted);
log("emptyReason=" + String(empty.reason));
log("emptyIsSignal=" + (empty instanceof AbortSignal));

// 2) a list containing an ALREADY aborted signal is aborted at construction,
//    with that signal's reason, and no abort event ever fires on it
const done = AbortSignal.abort("early");
const pending = new AbortController().signal;
const composed = AbortSignal.any([pending, done]);
composed.addEventListener("abort", function () { seen.push("composedLate"); });
log("composedAborted=" + composed.aborted);
log("composedReason=" + composed.reason);
log("composedListener=" + JSON.stringify(seen.join(",")));

// 3) the FIRST source to abort wins; the second is ignored
const a = new AbortController();
const b = new AbortController();
const race = AbortSignal.any([a.signal, b.signal]);
race.addEventListener("abort", function (ev: Event) {
  seen.push("raceAbort:" + ev.type + ":" + (ev.target === race));
});
log("raceBefore=" + race.aborted);
b.abort("b-first");
log("raceAfterB=" + race.aborted + " reason=" + race.reason);
a.abort("a-second");
log("raceAfterA reason=" + race.reason);
log("raceEvents=" + seen.filter(function (s) { return s.indexOf("raceAbort") === 0; }).join("|"));
log("sourceA=" + a.signal.reason + " sourceB=" + b.signal.reason);

// 4) the composite does NOT abort its sources
const c = new AbortController();
const d = new AbortController();
const comp = AbortSignal.any([c.signal, d.signal]);
c.abort("c-only");
log("compAborted=" + comp.aborted + " dUntouched=" + d.signal.aborted);

// 5) a signal may appear in several composites, and each gets the reason
const e = new AbortController();
const one = AbortSignal.any([e.signal]);
const two = AbortSignal.any([e.signal]);
log("twoCompositesDistinct=" + (one !== two));
e.abort("shared");
log("one=" + one.reason + " two=" + two.reason);

// 6) composites of composites propagate
const f = new AbortController();
const inner = AbortSignal.any([f.signal]);
const outer = AbortSignal.any([inner]);
log("nestedBefore=" + outer.aborted);
f.abort("deep");
log("nestedAfter=" + outer.aborted + " reason=" + outer.reason);

// 7) a duplicated source is harmless
const g = new AbortController();
const dup = AbortSignal.any([g.signal, g.signal]);
let dupCount = 0;
dup.addEventListener("abort", function () { dupCount++; });
g.abort("dup");
log("dupAborted=" + dup.aborted + " events=" + dupCount);

// 8) a non-signal member is refused
log("badMember=" + (function () {
  try { AbortSignal.any([42 as any]); return "no"; } catch (err: any) { return err.constructor.name; }
})());
log("nonIterable=" + (function () {
  try { AbortSignal.any(7 as any); return "no"; } catch (err: any) { return err.constructor.name; }
})());

// 9) the composite's own abort() is not exposed -- there is no controller
log("hasAbortMethod=" + (typeof (composed as any).abort));

// 10) throwIfAborted on the composite throws the propagated reason
const h = new AbortController();
const hi = AbortSignal.any([h.signal]);
h.abort(new RangeError("propagated"));
log("compositeThrow=" + (function () {
  try { hi.throwIfAborted(); return "no"; } catch (err: any) { return err.constructor.name; }
})());
log("compositeReasonIsSame=" + (hi.reason === h.signal.reason));

console.log("end");
