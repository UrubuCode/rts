import { io, promise, gc } from "rts";

io.print("=== RTS async/await/Promise/Function — DEMO ===\n");

// ==================== PARTE 1: async/await basico ====================
io.print("--- 1. async/await ---");

async function double(x: i64): i64 { return x * 2; }
async function compute(): i64 {
    const a = await double(5);     // 10
    const b = await double(10);    // 20
    return a + b;                  // 30
}

io.print("await double(21) = " + (await double(21)));        // 42
io.print("await compute() = " + (await compute()));          // 30

// ==================== PARTE 2: Promise direto ====================
io.print("\n--- 2. Promise primitivos ---");

const pres = promise.new_resolved(100);
io.print("resolved.wait = " + (await pres));                  // 100

const prej = promise.new_rejected(404);
let caught: i64 = -1;
try { await prej; } catch (e) { caught = 1; }
io.print("rejected caught = " + caught);                       // 1

// ==================== PARTE 3: Promise.then com user fn ====================
io.print("\n--- 3. Promise.then encadeado ---");

function add10(x: i64): i64 { return x + 10; }
function mul3(x: i64): i64 { return x * 3; }

const p = promise.new_resolved(5);
const r = await promise.then(promise.then(p, add10), mul3);
io.print("(5 + 10) * 3 = " + r);                              // 45

// ==================== PARTE 4: Function.call/apply/bind ====================
io.print("\n--- 4. Function class ---");

function add(a: i64, b: i64): i64 { return a + b; }
function multiply(a: i64, b: i64): i64 { return a * b; }

io.print("add.call(0, 3, 4) = " + add.call(0, 3, 4));         // 7
io.print("add.apply(0, [10, 20]) = " + add.apply(0, [10, 20])); // 30

const dbl = multiply.bind(0, 2);
io.print("dbl.call(0, 7) = " + dbl.call(0, 7));               // 14
io.print("multiply.length = " + multiply.length);             // 2
io.print("multiply.name = " + multiply.name);                 // multiply

// ==================== PARTE 5: new Function (eval em runtime) ====================
io.print("\n--- 5. new Function (compila em runtime) ---");

const sq = new Function("x", "return x * x;");
io.print("sq.call(0, 9) = " + sq.call(0, 9));                 // 81
io.print("sq.length = " + sq.length);                         // 1

const sumAll = new Function("a", "b", "c", "return a + b + c;");
io.print("sumAll(1,2,3) = " + sumAll.call(0, 1, 2, 3));       // 6

const ts = sumAll.toString();
io.print("source: " + ts);
gc.string_free(ts);

// ==================== PARTE 6: Promise + Function integrados ====================
io.print("\n--- 6. Promise.create (drysius design) ---");

// promise.create(fn, args) spawna fn em tokio task, retorna Promise
const pc1 = promise.create(add, [50, 50]);
io.print("create(add, [50,50]) = " + (await pc1));            // 100

const pc2 = promise.create(sq, [12]);
io.print("create(new Function, [12]) = " + (await pc2));      // 144

// ==================== PARTE 7: Function handle como callback ====================
io.print("\n--- 7. Function handle em promise.then ---");

const incBound = add.bind(0, 100);  // partial application
const p7 = promise.new_resolved(5);
io.print("then(p, add.bind(0,100)) = " + (await promise.then(p7, incBound))); // 105

const triple = new Function("x", "return x * 3;");
const p8 = promise.new_resolved(7);
io.print("then(p, new Function) = " + (await promise.then(p8, triple))); // 21

// ==================== PARTE 8: Rejection propaga via .call ====================
io.print("\n--- 8. throw em async + .call ---");

async function risky(x: i64): i64 {
    if (x > 10) throw 999;
    return x;
}

let r1: i64 = -1;
let r2: i64 = -1;
try { r1 = await risky.call(0, 5); } catch (e) { r1 = -100; }
try { r2 = await risky.call(0, 50); } catch (e) { r2 = -100; }
io.print("risky(5) = " + r1);                                  // 5
io.print("risky(50) = " + r2);                                 // -100

// ==================== PARTE 9: Stress concorrente ====================
io.print("\n--- 9. Stress: 50 async em sequencia ---");

async function inc(x: i64): i64 { return x + 1; }
let total: i64 = 0;
for (let i: i64 = 0; i < 50; i = i + 1) {
    total = total + (await inc(i));
}
io.print("sum(1..50) = " + total);                             // 1275

// ==================== PARTE 10: top-level await + composição ====================
io.print("\n--- 10. Composicao final ---");

async function pipeline(x: i64): i64 {
    const a = await double(x);                    // x*2
    const b = await promise.create(sq, [a]);      // (x*2)^2
    const c = mul3.call(0, b);                     // ((x*2)^2)*3
    return c;
}

io.print("pipeline(3) = " + (await pipeline(3))); // (6)^2 * 3 = 108

io.print("\n=== DEMO OK ===");
