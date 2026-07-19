let fired = 0;
function cb() { fired = 1; }
setTimeout(cb, 0);
console.log("perf-monotonic:" + (performance.now() >= 0));
// setTimeout callback drains in JIT event loop after main
Promise.resolve(0).then(() => { console.log("timer-fired:" + fired); });
