// What each action costs, in nanoseconds, measured the same way on every
// runtime that can run this file.
//
// # Why one file and no imports
//
// It is run under `rts`, under `node` and under `bun`, and the number that
// means anything is the RATIO between them: "string concatenation costs 40 ns"
// says nothing on its own, while "40 ns here and 6 ns on node" is a work item.
// An import would tie the measurement to one runtime's module resolution, which
// is one of the things being measured.
//
// # What it does not say
//
// Nothing about a program. Every case here runs one action in a loop with its
// operands already in hand, which is the best case for caches and the worst
// case for representativeness — a case that is fast here can still be slow in a
// program that reaches it once. It is an attribution instrument: it says which
// actions are expensive, not how much of any given program they are.
//
// # This file found an engine defect before it measured anything
//
// Its rows used to report roughly ten times what the same operation costs
// written on its own — `obj.a` at 225 ns here against 14 ns in a small file —
// and the harness was blamed for it in this comment. It was not the harness.
// `RTS_TIMING=1` reports inline-cache misses, and the two differed by five
// orders of magnitude: 75 against 1 135 690, one per iteration, which is a
// cached read resolving by name forever.
//
// The cause was `INLINE_SLOTS`: a property past the seventh is uncacheable,
// and a closure's environment is an object, so a script with more than seven
// captured bindings put an ordinary variable past the boundary. The constant
// is fifteen now and the rows below fell by up to 97%. Its documentation in
// `rts-core`'s `heap::region` has the whole trade, including what it cost.
//
// The cliff still exists at the SIXTEENTH property. A row here that looks
// absurd against node is still worth checking with `RTS_TIMING=1` before it is
// read as the cost of the operation.
//
// # Honesty
//
// Every case returns a number derived from its work and the harness sums it,
// so an optimiser removing the body shows up as a number too good to be true
// rather than as a fast one. The empty case measures the loop itself and is
// reported as the floor: a case at the floor is one whose cost the harness
// dominates, not a free action.

type Case = { group: string; name: string; ops: number; run: (n: number) => number };

const CASES: Case[] = [];
let SINK = 0;

function bench(group: string, name: string, run: (n: number) => number, ops?: number): void {
  CASES.push({ group, name, ops: ops === undefined ? 1 : ops, run: run });
}

// ---------------------------------------------------------------- the floor

bench("floor", "empty loop", (n) => {
  let acc = 0;
  for (let i = 0; i < n; i++) acc += i;
  return acc;
});

// ------------------------------------------------------------- arithmetic

bench("arith", "int add", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = a + 3;
  return a;
});
bench("arith", "int mul", (n) => {
  let a = 1;
  for (let i = 0; i < n; i++) a = (a * 3) | 0;
  return a;
});
bench("arith", "int div", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (i / 3) | 0;
  return a;
});
bench("arith", "int mod", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += i % 7;
  return a;
});
// The three shifts, and they are three rows rather than one because they do not
// cost the same thing and the difference is the point: `<<` and `>>` become
// instructions where the operands are proven doubles, while `>>>` uses its own
// logical-shift plus unsigned-widening sequence. Unknown operands still take the
// generic runtime fallback because their ToNumber conversion remains observable.
//
// There was NO shift row here at all until 2026-08-23, and that is why a 5x gap
// sat unseen: `(x << 3) >> 1` cost 1727 ms over 20 000 000 iterations against
// 341 ms for `(x & 255) | 1` in the same loop, because the shifts were runtime
// calls and the bitwise operators next to them were instructions. A row that
// does not exist measures nothing.
bench("arith", "int shl", (n) => {
  let a = 1;
  for (let i = 0; i < n; i++) a = (a << 3) | 0;
  return a;
});
bench("arith", "int shr", (n) => {
  let a = -1;
  for (let i = 0; i < n; i++) a = (a >> 1) | 0;
  return a;
});
bench("arith", "int shr unsigned", (n) => {
  let a = -1;
  for (let i = 0; i < n; i++) a = (a >>> 1) | 0;
  return a;
});
bench("arith", "float add", (n) => {
  let a = 0.5;
  for (let i = 0; i < n; i++) a = a + 1.25;
  return a | 0;
});
bench("arith", "float mul", (n) => {
  let a = 1.0000001;
  for (let i = 0; i < n; i++) a = a * 1.0000001;
  return a | 0;
});
bench("arith", "compare int", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (i < n) a++;
  return a;
});
// ---------------------------------------------------------------------------
// EVERY operator the language spells, whether or not anyone expects it to be
// slow.
//
// This block exists because of what its absence cost. There was no shift row
// here until 2026-08-23, and `<<`/`>>` were runtime calls while `&`/`|`/`^` next
// to them were instructions — a 5x gap that nothing could see, because the way
// you find out an operator is a call is by measuring it against one that is not.
//
// So the rule this block encodes: **an operator with no row is an operator
// nobody can find out about.** Coverage is the instrument; the rows that come
// out flat are not waste, they are the controls that make the others readable.
// `int shr unsigned` earned its place that way within a day. Its row now
// verifies that the logical-shift path remains observable instead of regressing
// to a generic call hidden behind the harness.
// ---------------------------------------------------------------------------
bench("arith", "int sub", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = (a - 1) | 0;
  return a;
});
bench("arith", "float sub", (n) => {
  let a = 1e9;
  for (let i = 0; i < n; i++) a = a - 1.25;
  return a | 0;
});
bench("arith", "float div", (n) => {
  let a = 1e30;
  for (let i = 0; i < n; i++) a = a / 1.0000001;
  return a | 0;
});
bench("arith", "exponent", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = (a + 2 ** 3) | 0;
  return a;
});
bench("arith", "int and", (n) => {
  let a = -1;
  for (let i = 0; i < n; i++) a = a & 255;
  return a;
});
bench("arith", "int or", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = a | 1;
  return a;
});
bench("arith", "int xor", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = a ^ 3;
  return a;
});
bench("arith", "int not", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = ~a;
  return a;
});
bench("arith", "negate", (n) => {
  let a = 1.5;
  for (let i = 0; i < n; i++) a = -a;
  return a | 0;
});
bench("arith", "unary plus", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = (a + +i) | 0;
  return a;
});
bench("arith", "logical not", (n) => {
  let a = 0;
  let f = false;
  for (let i = 0; i < n; i++) {
    f = !f;
    if (f) a++;
  }
  return a;
});
bench("arith", "strict equals int", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (i === n) a++;
  return a;
});
bench("arith", "loose equals int", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (i == n) a++;
  return a;
});
bench("arith", "Math.sqrt", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.sqrt(i);
  return a | 0;
});
bench("arith", "Math.floor", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.floor(i * 1.5);
  return a | 0;
});
bench("arith", "Math.random", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.random();
  return a | 0;
});

// ------------------------------------------------------------ calls, scope

function freeFn(x: number): number {
  return x + 1;
}
const arrowFn = (x: number): number => x + 1;
function varargsFn(...xs: number[]): number {
  return xs.length;
}
class Callee {
  v: number = 1;
  m(x: number): number {
    return x + this.v;
  }
}
const callee = new Callee();

bench("call", "free function", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = freeFn(a);
  return a;
});
bench("call", "arrow", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = arrowFn(a);
  return a;
});
bench("call", "method", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = callee.m(a);
  return a;
});
bench("call", "varargs 3", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += varargsFn(1, 2, 3);
  return a;
});
bench("call", "closure make+call", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const c = (x: number) => x + i;
    a = c(a) | 0;
  }
  return a | 0;
});
bench("call", "closure var read", (n) => {
  let outer = 1;
  const c = () => outer;
  let a = 0;
  for (let i = 0; i < n; i++) a += c();
  return a;
});

// ------------------------------------------------------------- properties

const obj = { a: 1, b: 2, c: 3, d: 4 };
class Base {
  bp(): number {
    return 1;
  }
}
class Derived extends Base {
  x: number = 1;
}
const derived = new Derived();
const keys = ["a", "b", "c", "d"];

bench("prop", "read own", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += obj.a;
  return a;
});
bench("prop", "write own", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) obj.a = i;
  a = obj.a;
  return a;
});
bench("prop", "read 4 fields", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += obj.a + obj.b + obj.c + obj.d;
  return a;
}, 4);
bench("prop", "computed key", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (obj as any)[keys[i & 3]];
  return a;
});
bench("prop", "proto method call", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += derived.bp();
  return a;
});
bench("prop", "optional chain", (n) => {
  let a = 0;
  const maybe: { a: number } | null = obj;
  for (let i = 0; i < n; i++) a += maybe?.a ?? 0;
  return a;
});
bench("prop", "in operator", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if ("a" in obj) a++;
  return a;
});
bench("prop", "typeof", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (typeof obj === "object") a++;
  return a;
});
// `typeof` with NOTHING compared against it. The case above measures
// `typeof x === "object"`, which is two operations — and string equality alone
// costs more than the whole line looks like it should, so reading that row as
// the cost of `typeof` overstates it by roughly a factor of two.
bench("prop", "typeof alone", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (typeof obj) a++;
  return a;
});
bench("prop", "instanceof", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (derived instanceof Base) a++;
  return a;
});

// ------------------------------------------------------------- allocation

bench("alloc", "object literal 2", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = { x: i, y: i };
    a += o.x;
  }
  return a;
});
bench("alloc", "object literal 8", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = { a: i, b: i, c: i, d: i, e: i, f: i, g: i, h: i };
    a += o.a;
  }
  return a;
});
bench("alloc", "class instance", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = new Callee();
    a += o.v;
  }
  return a;
});
bench("alloc", "array literal 4", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const xs = [i, i, i, i];
    a += xs[0];
  }
  return a;
});
bench("alloc", "add prop after", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o: any = { x: i };
    o.y = i;
    a += o.y;
  }
  return a;
});

// ------------------------------------------------------------------ arrays

const arr: number[] = [];
for (let i = 0; i < 1024; i++) arr.push(i);

bench("array", "index read", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += arr[i & 1023];
  return a;
});
bench("array", "index write", (n) => {
  for (let i = 0; i < n; i++) arr[i & 1023] = i;
  return arr[0];
});
bench("array", "push+pop", (n) => {
  const xs: number[] = [];
  let a = 0;
  for (let i = 0; i < n; i++) {
    xs.push(i);
    a += xs.pop() as number;
  }
  return a | 0;
}, 2);
bench("array", "for-of 16", (n) => {
  const xs = arr.slice(0, 16);
  let a = 0;
  for (let i = 0; i < n; i++) for (const x of xs) a += x;
  return a | 0;
}, 16);
bench("array", "map 16", (n) => {
  const xs = arr.slice(0, 16);
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.map((x) => x + 1)[0];
  return a;
}, 16);
bench("array", "filter 16", (n) => {
  const xs = arr.slice(0, 16);
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.filter((x) => x > 8).length;
  return a;
}, 16);
bench("array", "indexOf 16", (n) => {
  const xs = arr.slice(0, 16);
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.indexOf(15);
  return a;
}, 16);
bench("array", "join 16", (n) => {
  const xs = arr.slice(0, 16);
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.join(",").length;
  return a;
}, 16);

// ----------------------------------------------------------------- strings

const s16 = "abcdefghijklmnop";
const s256 = s16.repeat(16);

bench("string", "length", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.length;
  return a;
});
bench("string", "charCodeAt", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.charCodeAt(i & 15);
  return a;
});
bench("string", "index []", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16[i & 15].length;
  return a;
});
bench("string", "concat 2", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (s16 + "x").length;
  return a;
});
bench("string", "template literal", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += `v=${i}!`.length;
  return a;
});
bench("string", "equals", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (s16 === "abcdefghijklmnop") a++;
  return a;
});
bench("string", "indexOf 256", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s256.indexOf("mnop");
  return a;
});
bench("string", "slice 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s256.slice(0, 16).length;
  return a;
});
bench("string", "split 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += "a,b,c,d,e,f,g,h".split(",").length;
  return a;
});
bench("string", "toUpperCase 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += s16.toUpperCase().length;
  return a;
});
bench("string", "number->string", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += String(i).length;
  return a;
});
bench("string", "parseInt", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += parseInt("12345", 10);
  return a;
});
bench("string", "parseFloat", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += parseFloat("123.45") | 0;
  return a;
});

// ------------------------------------------------------------- collections

const map = new Map<string, number>();
const set = new Set<number>();
for (let i = 0; i < 64; i++) {
  map.set("k" + i, i);
  set.add(i);
}

bench("coll", "Map.get", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += map.get("k7") as number;
  return a;
});
bench("coll", "Map.set existing", (n) => {
  for (let i = 0; i < n; i++) map.set("k7", i);
  return map.get("k7") as number;
});
bench("coll", "Map.has", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (map.has("k7")) a++;
  return a;
});
bench("coll", "Set.has", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (set.has(7)) a++;
  return a;
});
bench("coll", "Object.keys 4", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Object.keys(obj).length;
  return a;
}, 4);

// -------------------------------------------------------------- json, regex

const jsonObj = { a: 1, b: "two", c: [1, 2, 3], d: { e: true } };
const jsonText = JSON.stringify(jsonObj);
const re = /[a-f]+([0-9]+)/;

bench("json", "stringify small", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += JSON.stringify(jsonObj).length;
  return a;
});
bench("json", "parse small", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (JSON.parse(jsonText) as any).a;
  return a;
});
bench("regex", "test", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (re.test("abc123")) a++;
  return a;
});
bench("regex", "exec+group", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const m = re.exec("abc123");
    a += m === null ? 0 : m[1].length;
  }
  return a;
});
bench("regex", "replace", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += "abc123".replace(re, "x").length;
  return a;
});

// ------------------------------------------------------- buffers, binaries

const u8 = new Uint8Array(1024);
const f64a = new Float64Array(256);
const dv = new DataView(u8.buffer);

bench("binary", "Uint8Array read", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += u8[i & 1023];
  return a;
});
bench("binary", "Uint8Array write", (n) => {
  for (let i = 0; i < n; i++) u8[i & 1023] = i & 255;
  return u8[0];
});
bench("binary", "Float64Array rw", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    f64a[i & 255] = i;
    a += f64a[i & 255];
  }
  return a | 0;
}, 2);
bench("binary", "DataView getU32", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += dv.getUint32(0, true);
  return a | 0;
});
bench("binary", "alloc Uint8Array 64", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += new Uint8Array(64).length;
  return a;
});
bench("binary", "subarray 64", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += u8.subarray(0, 64).length;
  return a;
});
bench("binary", "TextEncoder 16", (n) => {
  const enc = new TextEncoder();
  let a = 0;
  for (let i = 0; i < n; i++) a += enc.encode(s16).length;
  return a;
});

// ------------------------------------------------- control flow, exceptions

bench("flow", "try/catch no throw", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    try {
      a += 1;
    } catch {
      a -= 1;
    }
  }
  return a;
});
bench("flow", "throw+catch", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    try {
      throw new Error("x");
    } catch {
      a += 1;
    }
  }
  return a;
});
bench("flow", "generator next", (n) => {
  function* g(): Generator<number> {
    for (;;) yield 1;
  }
  const it = g();
  let a = 0;
  for (let i = 0; i < n; i++) a += it.next().value as number;
  return a;
});
bench("flow", "switch 8-way", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    switch (i & 7) {
      case 0: a += 1; break;
      case 1: a += 2; break;
      case 2: a += 3; break;
      case 3: a += 4; break;
      case 4: a += 5; break;
      case 5: a += 6; break;
      case 6: a += 7; break;
      default: a += 8; break;
    }
  }
  return a;
});

// ------------------------------------------------------------------ harness

// Nanoseconds a case costs per action, with the loop that carried it removed.
//
// The count is CALIBRATED rather than fixed: a case at 2 ns and a case at 2 us
// share this harness, and one iteration count cannot serve both — too few and
// the clock's own resolution is the number, too many and the slow cases take
// minutes. So each case is grown until it takes at least `TARGET_MS`.
const TARGET_MS = 40;
const WARMUP = 1;

function timeOnce(c: Case, n: number): number {
  const t0 = performance.now();
  SINK += c.run(n);
  return performance.now() - t0;
}

type Row = { group: string; name: string; nanos: number; failed: string };

function measure(c: Case): Row {
  try {
    let n = 1024;
    let ms = timeOnce(c, n);
    // Growing by a factor rather than by extrapolation: a case whose cost is
    // not linear in `n` (an array that grows, a string that accumulates) would
    // have the extrapolation overshoot by orders of magnitude.
    //
    // # Why the factor is two and was four
    //
    // The factor is the OVERSHOOT. With four, the first count to clear
    // `TARGET_MS` lands anywhere in [40, 160) ms, so the three timed runs that
    // follow cost 120 to 480 ms and average about 440 ms per case. With two the
    // window is [40, 80) and the average halves.
    //
    // Precision does not pay for it, and that is the whole argument: what sets
    // precision is `TARGET_MS` — the floor of the measurement window — and both
    // factors clear the same floor. Four buys a *longer* window than asked for,
    // at random, depending on where the threshold happens to fall.
    //
    // Measured 2026-08-23, this file at 90 cases: **32 s at four, 26 s at two**.
    // Of 91 rows, ONE moved by more than 8% between the two factors — and the
    // same harness compared against ITSELF across two runs moves four. The
    // change is below the noise it would have to beat to be visible.
    //
    // The one row that moved is `string concat 2`, which is the class this
    // function's first paragraph names: its cost is not linear in `n`, so its
    // ns/op genuinely depends on the count, and both numbers are correct for
    // the count they were taken at. Two is the better factor for those as well,
    // because the count it settles on is nearer the smallest one that clears the
    // floor and therefore varies less between runs.
    while (ms < TARGET_MS && n < 1 << 26) {
      n = n * 2;
      ms = timeOnce(c, n);
    }
    for (let w = 0; w < WARMUP; w++) ms = timeOnce(c, n);
    let best = ms;
    for (let r = 0; r < 2; r++) {
      const again = timeOnce(c, n);
      if (again < best) best = again;
    }
    return { group: c.group, name: c.name, nanos: (best * 1e6) / (n * c.ops), failed: "" };
  } catch (e) {
    // A case that throws is DATA, not an interruption: an action this engine
    // does not have is exactly what an analytic of what it costs should report,
    // and stopping at the first one would report nothing about anything after.
    return { group: c.group, name: c.name, nanos: 0, failed: String(e).slice(0, 60) };
  }
}

function pad(s: string, w: number): string {
  let out = s;
  while (out.length < w) out = out + " ";
  return out;
}

function padLeft(s: string, w: number): string {
  let out = s;
  while (out.length < w) out = " " + out;
  return out;
}

const rows: Row[] = [];
for (const c of CASES) rows.push(measure(c));

const floor = rows[0].nanos;
console.log("action                          ns/op    minus floor");
console.log("------------------------------------------------------");
for (const r of rows) {
  const label = pad(r.group + " " + r.name, 30);
  if (r.failed !== "") {
    console.log(label + "  UNAVAILABLE  " + r.failed);
    continue;
  }
  const net = r.nanos - floor;
  console.log(
    label +
      padLeft(r.nanos.toFixed(2), 9) +
      padLeft(net > 0 ? net.toFixed(2) : "~0", 13),
  );
}
console.log("------------------------------------------------------");
console.log("floor (empty loop iteration): " + floor.toFixed(2) + " ns");
console.log("checksum " + SINK);
