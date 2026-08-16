// Cross-runtime: how many microtask TICKS an `await` costs, measured against a
// ruler chain. Focus: a plain value, a native promise, a subclass instance and
// a thenable each resume at a DIFFERENT tick, and the spread is specified.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

// The ruler: a chain of ten already-scheduled ticks. Each link runs one
// microtask later than the previous one, so a resumption printed between
// "tick3" and "tick4" resumed on tick 4.
let ruler: Promise<void> = Promise.resolve();
for (let i = 1; i <= 10; i++) {
  const k = i;
  ruler = ruler.then(function () { log("tick" + k); });
}

// 1) awaiting a non-promise value
(async function () {
  await 1;
  log("resumed: plain value");
})();

// 2) awaiting an already-fulfilled native promise
(async function () {
  await Promise.resolve(2);
  log("resumed: native promise");
})();

// 3) awaiting a thenable object (an extra job to call `then`, then the job
//    that runs the resolve function)
(async function () {
  await { then: function (r: any) { r(3); } };
  log("resumed: thenable");
})();

// 4) awaiting a promise whose `then` is inherited from a Promise SUBCLASS,
//    which is not the fast path a bare native promise takes
class MyP extends Promise {}
(async function () {
  await MyP.resolve(4);
  log("resumed: subclass promise");
})();

// 5) two awaits in a row cost their ticks one after the other
(async function () {
  await 5;
  log("resumed: double await, first");
  await 5;
  log("resumed: double await, second");
})();

// 6) `.then` on an already-fulfilled promise is one tick, for comparison
Promise.resolve(6).then(function () { log("resumed: bare then"); });

// 7) a return value that is a promise costs the async function extra ticks
//    before its caller sees it
(async function () {
  return Promise.resolve(7);
})().then(function (v: any) { log("resumed: returned promise value=" + v); });

// 8) sync part runs before ANY of the above
log("synchronous tail");

// 9) the last word, well past the ruler
let far: Promise<void> = ruler;
for (let i = 0; i < 4; i++) far = far.then(function () { return undefined; });
far.then(function () { console.log("end"); });
