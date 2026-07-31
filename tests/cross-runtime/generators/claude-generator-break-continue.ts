// Cross-runtime: `break` / `continue` inside a generator loop.
//
// `break` used to HANG FOREVER — no error, no output. The state machine turns a
// loop body into STATES, and a `break` emitted verbatim inside a state has no
// loop to leave, so the cut was ignored and the generator ran forever.
//
// Such a body is now ineligible for the state machine and falls back to the
// eager buffer, which keeps the body verbatim and therefore honours the cut.
// Laziness is lost for those bodies; termination is gained.

function* breakWhile() { let i = 0; while (true) { if (i >= 2) break; yield i; i = i + 1; } }
console.log("breakWhile=" + [...breakWhile()].join(","));

function* breakFor() { for (let i = 0; i < 5; i++) { if (i === 2) break; yield i; } }
console.log("breakFor=" + [...breakFor()].join(","));

function* continueForOf() { for (const x of [1, 2, 3]) { if (x === 2) continue; yield x; } }
console.log("continueForOf=" + [...continueForOf()].join(","));

function* breakDoWhile() { let i = 0; do { if (i >= 2) break; yield i; i = i + 1; } while (true); }
console.log("breakDoWhile=" + [...breakDoWhile()].join(","));

function* continueWhile() { let i = 0; while (i < 4) { i = i + 1; if (i % 2 === 0) continue; yield i; } }
console.log("continueWhile=" + [...continueWhile()].join(","));

// `.next()` walks the same values one at a time
const it = breakWhile();
console.log("next=" + it.next().value + "," + it.next().value + "," + JSON.stringify(it.next()));

// ── non-regressions: what must stay LAZY ────────────────────────────────────
function* infinite() { let i = 0; while (true) { yield i; i = i + 1; } }
const ii = infinite();
console.log("infiniteStaysLazy=" + ii.next().value + "," + ii.next().value);

function* withSent() { let i = 0; while (i < 3) { const v = yield i; i = i + (v || 1); } }
const is = withSent();
console.log("sentValue=" + is.next().value + "," + is.next(2).value);

function* noBreak() { let i = 0; while (i < 2) { yield i; i = i + 1; } }
console.log("noBreak=" + [...noBreak()].join(","));
