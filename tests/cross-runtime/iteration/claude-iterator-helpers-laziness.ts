// Cross-runtime: the ES2025 iterator helpers are LAZY. Building a chain pulls
// nothing; each next() pulls exactly what it needs; and take() closes the source
// the moment its budget runs out. The pull log is the assertion.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

function* naturals(tag: string) {
  try {
    let i = 0;
    while (true) {
      pulls.push(tag + i);
      yield i++;
    }
  } finally {
    pulls.push(tag + "-closed");
  }
}

// 1) building the chain pulls nothing at all
pulls.length = 0;
const chain = naturals("a").map(function (x: number) { return x * 10; }).filter(function (x: number) { return x > 0; }).take(3);
log("1 pullsAfterBuild=" + JSON.stringify(pulls.join(",")));

// 2) one next() pulls only as far as it must -- filter rejects a0=0, so the
//    first value costs two pulls
log("2 first=" + JSON.stringify(chain.next()));
log("2 pulls=" + pulls.join(","));
log("2 second=" + JSON.stringify(chain.next()));
log("2 pulls=" + pulls.join(","));

// 3) draining a take(3) closes the source right after the third value
log("3 third=" + JSON.stringify(chain.next()));
log("3 fourth=" + JSON.stringify(chain.next()));
log("3 pulls=" + pulls.join(","));
log("3 fifth=" + JSON.stringify(chain.next()));

// 4) take(0) closes the source without pulling anything
pulls.length = 0;
const zero = naturals("b").take(0);
log("4 result=" + JSON.stringify(zero.next()));
log("4 pulls=" + pulls.join(","));

// 5) drop(k) discards eagerly on the first pull, not at construction
pulls.length = 0;
const dropped = naturals("c").drop(2);
log("5 afterBuild=" + JSON.stringify(pulls.join(",")));
log("5 first=" + dropped.next().value);
log("5 pulls=" + pulls.join(","));
(dropped as any).return();
log("5 afterClose=" + pulls.join(","));

// 6) an infinite source is safe behind take, and toArray drains exactly it
pulls.length = 0;
log("6 toArray=" + naturals("d").take(4).toArray().join(","));
log("6 pulls=" + pulls.join(","));

// 7) flatMap is lazy in both dimensions
pulls.length = 0;
const flat = naturals("e").take(3).flatMap(function (x: number) { return [x, x]; });
log("7 afterBuild=" + JSON.stringify(pulls.join(",")));
log("7 a=" + flat.next().value + " pulls=" + pulls.join(","));
log("7 b=" + flat.next().value + " pulls=" + pulls.join(","));
log("7 rest=" + flat.toArray().join(","));
log("7 pulls=" + pulls.join(","));

// 8) closing a helper closes the whole chain beneath it
pulls.length = 0;
const deep = naturals("f").map(function (x: number) { return x; }).filter(function () { return true; });
deep.next();
log("8 beforeReturn=" + pulls.join(","));
log("8 returned=" + JSON.stringify((deep as any).return("bye")));
log("8 afterReturn=" + pulls.join(","));
log("8 nextAfterReturn=" + JSON.stringify(deep.next()));

// 9) a helper over a finite source ends when the source does
pulls.length = 0;
function* finite() { try { yield 1; yield 2; } finally { pulls.push("finite-closed"); } }
const fin = finite().map(function (x: number) { return x + 100; });
log("9 a=" + JSON.stringify(fin.next()));
log("9 b=" + JSON.stringify(fin.next()));
log("9 c=" + JSON.stringify(fin.next()));
log("9 pulls=" + pulls.join(","));

// 10) a helper is single-use and is its own iterable
const once = finite().map(function (x: number) { return x; });
log("10 selfIterable=" + ((once as any)[Symbol.iterator]() === once));
log("10 firstPass=" + once.toArray().join(","));
log("10 secondPass=" + JSON.stringify(once.toArray().join(",")));

console.log("end");
