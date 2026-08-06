// Functions: calling, closing over, binding, and the argument vector.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

function plain(a, b) { return a + b; }
check("call", plain(1, 2) === 3);
check("missing-argument", isNaN(plain(1)));
check("typeof", typeof plain === "function");
check("identity", plain === plain);
check("is-an-object", (function () { plain.tag = 1; return plain.tag === 1; })());
check("has-prototype", typeof plain.prototype === "object");

let expression = function (x) { return x * 2; };
check("expression", expression(2) === 4);
let arrow = function (x) { return x + 1; };
check("arrow-like", arrow(1) === 2);

// A closure reads the variable, not a copy of it at capture time.
check("closure", (function () {
    let n = 1;
    function read() { return n; }
    n = 2;
    return read() === 2;
})());
check("closure-writes", (function () {
    let n = 1;
    function bump() { n = n + 1; }
    bump();
    bump();
    return n === 3;
})());
check("closure-two-out", (function () {
    let n = 5;
    function outer() { function inner() { return n; } return inner(); }
    return outer() === 5;
})());
check("recursion", (function () {
    function down(n) { if (n === 0) { return 0; } return n + down(n - 1); }
    return down(3) === 6;
})());

// The receiver.
let holder = {n: 4, read: function () { return this.n; }};
check("receiver", holder.read() === 4);
check("no-receiver", (function () {
    let loose = holder.read;
    return loose() === undefined;
})());

check("call-method", plain.call(null, 1, 2) === 3);
check("apply-method", plain.apply(null, [1, 2]) === 3);
check("call-receiver", holder.read.call({n: 9}) === 9);
check("apply-receiver", holder.read.apply({n: 9}) === 9);

// `bind` fixes the receiver, and the bound one wins over the call's.
let bound = holder.read.bind({n: 7});
check("bind", bound() === 7);
check("bind-wins", (function () {
    let other = {n: 1, m: bound};
    return other.m() === 7;
})());
check("bind-partial", plain.bind(null, 1)(2) === 3);
check("bind-nothing-prepended", (function () {
    function one(a) { return a; }
    return one.bind(null)(9) === 9;
})());
// Binding twice keeps the first receiver, because the second binds the
// already-bound function.
check("bind-twice", holder.read.bind({n: 3}).bind({n: 4})() === 3);

// A rest parameter over four or fewer arguments has to work anyway, and a
// spread pushes past the convention's four slots.
check("rest-few", (function () {
    function tail(a, ...rest) { return rest.length; }
    return tail(1, 2, 3) === 2;
})());
check("rest-none", (function () {
    function tail(a, ...rest) { return rest.length; }
    return tail(1) === 0;
})());
check("spread-call", (function () {
    function three(a, b, c) { return a + b + c; }
    return three(...[1, 2, 3]) === 6;
})());
check("spread-past-four", (function () {
    function count(...all) { return all.length; }
    return count(...[1, 2, 3, 4, 5, 6]) === 6;
})());

// A callee does not see an outer call's argument vector.
check("vector-isolation", (function () {
    function inner(...rest) { return rest.length; }
    function outer(...rest) { return inner(1); }
    return outer(1, 2, 3, 4, 5) === 1;
})());

check("inherits-call", typeof plain.call === "function");
check("inherits-apply", typeof plain.apply === "function");
check("inherits-bind", typeof plain.bind === "function");

// A `var` declared inside a `try` reaches the rest of its function. It did not:
// `declared_by_statement` had no `Try` arm at all, so the name never joined the
// set the environment is built from — and a name assigned under protection that
// is not in that set is dropped by the intersection. The program did not
// compile: the read was an unbound name.
check("var-in-try-seen-by-finally", (function () {
    let seen = 0;
    function run() { try { var x = 5; } finally { seen = x; } }
    run();
    return seen === 5;
})());
check("var-in-try-seen-after", (function () {
    function run() { try { var x = 7; } finally { } return x; }
    return run() === 7;
})());
check("var-in-try-captured", (function () {
    function run() { try { var x = 3; } finally { } return (function () { return x; })(); }
    return run() === 3;
})());
check("var-in-catch-binding", (function () {
    function run() { try { var x = 1; } catch (e) { x = 2; } return x; }
    return run() === 1;
})());

return failed;
