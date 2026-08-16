// Cross-runtime: `await` must PARK the frame, not run the rest of the body.
//
// It did not. `await` lowered to a call that drained the microtask queue until
// the promise settled, so the awaiting frame kept the machine and everything
// after the `await` ran BEFORE the caller's next statement. The values were
// right and the interleaving was not, which is the one thing an async function
// exists to decide.
//
// Every line below is about ORDER. None of them can pass by computing the right
// value at the wrong time.

async function f() {
  console.log("1 body-start");
  await Promise.resolve();
  console.log("3 after-await");
}

f();
console.log("2 caller-continues");

// A `.then` attached AFTER the await parked runs after the resumption: the two
// kinds of waiter share one queue, in the order they attached.
Promise.resolve().then(function () { console.log("4 then"); });

// The promise an async function answers settles when the BODY ends, not when
// the call returns.
async function g() {
  await Promise.resolve();
  return "value";
}
g().then(function (v) { console.log("5 settled-with " + v); });

// A rejection crossing an `await` is a throw at the `await`, inside the `try`
// it was written in.
async function h() {
  try {
    await Promise.reject(new Error("boom"));
    console.log("unreachable");
  } catch (e) {
    console.log("6 caught " + (e as Error).message);
  }
  return 1;
}
h().then(function (v) { console.log("7 recovered " + v); });

// A throw that escapes the body REJECTS the promise rather than ending the
// program.
async function thrower() {
  throw new Error("escaped");
}
thrower().catch(function (e) { console.log("8 rejected " + (e as Error).message); });

// Awaiting in a loop suspends every pass, and the accumulated value survives
// each suspension — which is what the frame rewrite spills.
async function counting() {
  let total = 0;
  for (let i = 1; i <= 3; i++) {
    total += await Promise.resolve(i);
  }
  console.log("9 total " + total);
}
counting();
