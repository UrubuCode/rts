// `Promise` — reached through `.then`, never through `await`, which the
// emitter refuses by name.
//
// A fixture cannot observe a reaction directly: the microtask queue drains
// after the program returns, so anything a `.then` writes is written too late
// for this file to check. What it CAN check is everything that happens
// synchronously — which is the half most likely to be got wrong, because a
// reaction that ran early would be visible right here.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("is-object", typeof Promise.resolve(1) === "object");
check("has-then", typeof Promise.resolve(1).then === "function");
check("has-catch", typeof Promise.resolve(1).catch === "function");
check("has-finally", typeof Promise.resolve(1).finally === "function");
check("then-answers-a-promise", typeof Promise.resolve(1).then(function () {}).then === "function");
check("instance-of", Promise.resolve(1) instanceof Promise);

// The single most-tested property of a promise implementation: a reaction is a
// microtask, so it must not have run by the time `then` returns.
let ran = false;
Promise.resolve(1).then(function () { ran = true; });
check("not-synchronous", ran === false);

// The executor, by contrast, runs SYNCHRONOUSLY — the pair that separates a
// correct implementation from one that queues everything.
let executed = false;
let built = new Promise(function (resolve, reject) { executed = true; resolve(1); });
check("executor-is-synchronous", executed === true);
check("constructed-is-a-promise", built instanceof Promise);

check("resolve-existing", (function () {
    let p = Promise.resolve(1);
    return Promise.resolve(p) === p;
})());

check("all-is-a-promise", Promise.all([Promise.resolve(1)]) instanceof Promise);
check("all-settled-is-a-promise", Promise.allSettled([]) instanceof Promise);
check("race-is-a-promise", Promise.race([Promise.resolve(1)]) instanceof Promise);
check("any-is-a-promise", Promise.any([Promise.resolve(1)]) instanceof Promise);
check("reject-is-a-promise", Promise.reject(1).catch(function () {}) instanceof Promise);

// A rejection with a handler attached is not unhandled, which is what the
// report at the end of the turn keys on.
Promise.reject(new Error("caught")).catch(function () {});

return failed;
