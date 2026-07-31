// Cross-runtime: `try { ...yield... } catch (e) { ...no yield... }` — the common
// shape — inside a generator.
//
// It did not compile ("expression raw/unrecognized: Yield"): the state machine's
// try/catch path required the yield to be IN THE CATCH, so a try that suspends
// with an ordinary catch fell back to the eager buffer, which cannot express a
// value-position yield.
//
// The machinery was already right (ENTER_TRY_CATCH → body → CAUGHT → catch); only
// the entry condition was too narrow. A catch without yield lowers as ordinary
// statements inside the catch state.

function* emptyCatch() { try { const v = yield 1; yield v; } catch (e) { } }
const ia = emptyCatch();
console.log("emptyFirst=" + ia.next().value);
console.log("emptySent=" + ia.next(5).value);

function* catchWithYield() { try { const v = yield 1; yield v; } catch (e) { yield 9; } }
const ib = catchWithYield();
ib.next();
console.log("catchYieldSent=" + ib.next(5).value);

function* continuesAfter() { try { const v = yield 1; yield v * 2; } catch (e) { } yield 99; }
const id = continuesAfter();
console.log("after=" + id.next().value + "," + id.next(3).value + "," + id.next().value);

function* catchBodyNotRun() {
  let mark = 0;
  try { const v = yield 1; yield v; } catch (e) { mark = 1; }
  yield mark;
}
const ie = catchBodyNotRun();
ie.next();
ie.next(7);
console.log("catchNotRun=" + ie.next().value);

// the full result object survives
console.log("resultObject=" + JSON.stringify(emptyCatch().next()));

// ── non-regressions ─────────────────────────────────────────────────────────
function* tryFinally() { try { yield 1; } finally { } yield 2; }
console.log("tryFinally=" + [...tryFinally()].join(","));

function* noTry() { const a = yield 1; yield a * 3; }
const st = noTry();
console.log("noTry=" + st.next().value + "," + st.next(4).value);
