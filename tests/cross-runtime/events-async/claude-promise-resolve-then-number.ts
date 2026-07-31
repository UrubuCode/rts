// Cross-runtime: `Promise.resolve(n).then(cb)` must hand the NUMBER to the
// callback, not the raw f64 bits.
//
// It delivered `4619567317775286272` for `7` — exactly the f64 bit pattern of
// 7.0. Only numbers were affected; string/bool/object/array were fine. And the
// SAME value read through `await` was correct, which is what pinned the cause: a
// value with no handle (number/bool/null) IS its PolyValue word, but the slot was
// marked as NOT-a-word, so `.then` forwarded the raw bits. `await` escaped it by
// heuristically converting f64 bits back to an integer.

Promise.resolve(7).then(function (v) { console.log("num=" + v); });
Promise.resolve(-42).then(function (v) { console.log("neg=" + v); });
Promise.resolve(3.5).then(function (v) { console.log("frac=" + v); });
Promise.resolve(0).then(function (v) { console.log("zero=" + v); });

// the value must be USABLE, not merely printable
Promise.resolve(5).then(function (v) { return v * 2; }).then(function (v) {
  console.log("chain=" + v);
});

// ── non-regressions: the types that already worked ──────────────────────────
Promise.resolve(true).then(function (v) { console.log("bool=" + v); });
Promise.resolve(null).then(function (v) { console.log("null=" + v); });
Promise.resolve("oi").then(function (v) { console.log("str=" + v); });
Promise.resolve({ a: 1 }).then(function (v) { console.log("obj=" + JSON.stringify(v)); });
Promise.resolve([1, 2]).then(function (v) { console.log("arr=" + v.join(",")); });

// `await` over the same value stays correct
async function viaAwait() { return await Promise.resolve(11); }
viaAwait().then(function (v) { console.log("await=" + v); });
