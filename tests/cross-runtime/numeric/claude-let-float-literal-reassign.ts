// Cross-runtime: a `let` initialized with an int-valued float literal (0.0/20.0)
// must not truncate later fractional reassignments (issue #1869).

// direct fractional reassign
let py = 20.0;
py = 10 + 2.7;
console.log("py=" + py);

// self-referential fractional op (sign must survive too)
let vy = 0.0;
vy = vy - 0.272;
console.log("vy=" + vy);

// compound assignment with a fractional literal
let acc = 0.0;
acc += 0.5;
acc -= 0.1;
console.log("acc=" + acc);

// annotated control
let ok: number = 20.0;
ok = 10 + 2.7;
console.log("ok=" + ok);

// non-integer initializer (Float from the start)
let yaw = 0.7;
yaw = yaw + 0.05;
console.log("yaw=" + yaw);

// integer-only loop must STAY integer (no regression to the fast path):
// only integer-valued literals are ever assigned, so no float promotion.
let a = 0.0;
let b = 1.0;
let i = 0.0;
while (i < 10.0) {
  let t = a + b;
  a = b;
  b = t;
  i = i + 1.0;
}
console.log("fib=" + a);
