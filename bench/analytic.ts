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
// **The cliff has moved, and this paragraph used to point at the wrong half of
// it.** It said the cliff still exists at the sixteenth property, meaning
// READS. Measured 2026-09-06 by the four `ic read slot …` rows added below,
// that is no longer true: slot 0, slot 14, slot 15 and slot 31-of-32 all read
// at 2.90–3.08 ns, which is one number, not a cliff.
//
// What DOES fall off is CONSTRUCTION, and by more than anything else in this
// file: `alloc object literal 8 escaping` is 57.9 ns and `alloc object literal
// 16 escaping` is **7 473 ns** — 129×, against bun's 6.4 → 13.1. So a row that
// looks absurd is still worth checking with `RTS_TIMING=1`, but the thing to
// suspect is the overflow path of the WRITE, not a cached read that refused.
//
// # The 500-line ceiling does not reach this file, and that is a DECISION
//
// This file is over two thousand lines. Agreed 2026-09-06: **the ceiling binds
// the system's own code and not a test or benchmark corpus.** It is written
// here rather than assumed, because RULE 0 says a rule the code contradicts is
// changed with its reason rather than left standing — and because the next
// person to count the lines will otherwise file it as debt.
//
// Two reasons it is the right line to draw, and the first is specific to this
// file. It must run unmodified under `rts`, under `node` and under `bun`, so it
// can have no imports, and a benchmark corpus with no imports is one file or it
// is nothing — splitting it into a folder would tie every measurement to one
// runtime's module resolution, which is one of the things being measured.
//
// The second is general, and is why the rule reads the way it now does. What
// the ceiling protects is that a change lands in a small focused module rather
// than being appended to something already oversized — a claim about coupling.
// A corpus has none: it is a list of independent cases, and case 200 cannot be
// made worse by case 199 existing. Length here is coverage, which is the thing
// the file is for.
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

// ===========================================================================
// PART TWO — added 2026-09-06
//
// # Why it is appended rather than merged into the sections above
//
// Every row above has a number recorded against its name somewhere in
// `docs/codegen/`. Renaming one, or moving it so the table's order changes,
// silently invalidates those records — so nothing above this line was touched,
// and the rows below are new names only. A number for `prop read own` taken
// today is still comparable with the one in `plan.md`.
//
// # The rule these cases follow, and the defect it avoids
//
// **A fixture belongs INSIDE its case body, hoisted above the loop.** Not at
// module scope. The header of this file records why in the only terms that
// matter here: a closure's environment is an object, so every module-level
// binding a case captures is a property of one object, and the property past
// the fifteenth is uncacheable. This file already reported rows ten times too
// high once for exactly that reason. Doubling the number of cases by doubling
// the number of module-level fixtures would have re-created it, and the damage
// would have landed on the rows above — which is to say, on the records.
//
// Creating the fixture inside `run(n)` costs one construction per measured
// call, against `n` iterations of the loop. At the counts this harness settles
// on it is not visible.
//
// Three module-level bindings are added and no more, each because a case needs
// a value the compiler must NOT be able to prove:
//
//   KEEP          — an escape hatch. Assigning to it is the cheapest way to
//                   stop an allocation being scalar-replaced away, which is
//                   what `plan.md` §7.1 records as the reason the two
//                   `alloc object literal` rows above measure nothing.
//   ESCAPED_MASK  — an integer the emitter cannot fold.
//   ESCAPED_SEED  — a double that reaches an operation through a capture
//                   rather than as a loop local.
//
// The last two are the instrument for `docs/codegen/the-missing-pass.md`, and
// that is the single largest thing this file could not see before today.
// ===========================================================================

let KEEP: any = null;
let ESCAPED_MASK = 1023;
let ESCAPED_SEED = 1.0000001;

// ------------------------------------------- the machine-operation frontier
//
// `emit/call.rs`'s `machine_operation` turns `Math.floor(x)` into the
// instruction the hardware has on three conditions, the third being that the
// operand is ALREADY a proven double. `docs/codegen/the-missing-pass.md`
// measures what happens when it is not — one instruction becomes a full
// JavaScript call — and states the reason plainly: provenness does not survive
// a block boundary, because the pass that would carry it across one does not
// exist.
//
// Each pair below is the same operation twice, differing ONLY in where its
// operand comes from. The difference between the two rows is the whole of what
// that missing pass is worth on that operation. On a runtime whose optimiser
// does not care where a value came from, the two rows are the same number —
// which is what makes this readable as a table and not as an assertion.

bench("machine", "Math.floor proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.floor(i * 1.5);
  return a | 0;
});
bench("machine", "Math.floor captured", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    ESCAPED_SEED = ESCAPED_SEED * 1.0000001;
    a += Math.floor(ESCAPED_SEED);
  }
  return a | 0;
});
bench("machine", "Math.sqrt proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.sqrt(i * 1.5);
  return a | 0;
});
bench("machine", "Math.sqrt captured", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    ESCAPED_SEED = ESCAPED_SEED * 1.0000001;
    a += Math.sqrt(ESCAPED_SEED);
  }
  return a | 0;
});
bench("machine", "Math.abs proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.abs(i - 500);
  return a | 0;
});
bench("machine", "Math.min proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.min(i, 7);
  return a | 0;
});
bench("machine", "Math.max proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.max(i & 7, 3);
  return a | 0;
});
bench("machine", "Math.round proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.round(i * 1.5);
  return a | 0;
});
bench("machine", "Math.trunc proven", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.trunc(i * 1.5);
  return a | 0;
});
// The other side of the frontier: `Math` members with no machine form at all.
// They are here as the control that says what a `Math` call costs when the
// instruction is not the question, so a slow `Math.floor captured` cannot be
// read as "Math is slow".
bench("machine", "Math.pow", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.pow(i & 7, 2);
  return a | 0;
});
bench("machine", "Math.log", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.log(i + 1);
  return a | 0;
});
bench("machine", "Math.sin", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.sin(i);
  return a | 0;
});
bench("machine", "Math.hypot", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.hypot(i, 3);
  return a | 0;
});
bench("machine", "Math.imul", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a = Math.imul(a + i, 3) | 0;
  return a;
});
bench("machine", "Math.fround", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.fround(i * 1.5);
  return a | 0;
});
bench("machine", "Math.clz32", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Math.clz32(i);
  return a | 0;
});
// `ToInt32` over a constant, against `ToInt32` over a value the emitter cannot
// fold. `the-missing-pass.md` prices the first at 3.08 ns per occurrence on an
// isolated model — "roughly twice what the entire loop costs when nothing is in
// its way" — and this is the pair that says whether that reaches a program.
bench("machine", "mask by literal", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += i & 1023;
  return a;
});
bench("machine", "mask by captured", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += i & ESCAPED_MASK;
  return a;
});
bench("machine", "float add proven", (n) => {
  let a = 0.5;
  for (let i = 0; i < n; i++) a = a + i * 0.5;
  return a | 0;
});
bench("machine", "float add via property", (n) => {
  const box = { k: 0.5 };
  KEEP = box;
  let a = 0.5;
  for (let i = 0; i < n; i++) a = a + box.k;
  return a | 0;
});
bench("machine", "float add via array", (n) => {
  const xs = [0.5, 0.25, 0.125, 0.0625];
  KEEP = xs;
  let a = 0.5;
  for (let i = 0; i < n; i++) a = a + xs[i & 3];
  return a | 0;
});

// ------------------------------------------------------ inline-cache shapes
//
// A property read at one site over one shape is the case every engine is fast
// at. What separates them is what happens when the site sees two shapes, or
// four, or more than a cache can hold.
//
// **Read these rows as DIFFERENCES, never alone.** Every one of them pays the
// same `objs[i & k]` element load, and `array index read` above says that is
// not free here. `ic mono (control)` is that cost with one shape; each row
// after it adds only shapes. The control is the subtrahend and the file has no
// other way to give you one.

bench("ic", "mono (control)", (n) => {
  const objs = [{ v: 1 }, { v: 2 }, { v: 3 }, { v: 4 }];
  KEEP = objs;
  let a = 0;
  for (let i = 0; i < n; i++) a += objs[i & 3].v;
  return a;
});
bench("ic", "poly 2", (n) => {
  const objs: any[] = [{ v: 1 }, { x: 0, v: 2 }, { v: 3 }, { x: 0, v: 4 }];
  KEEP = objs;
  let a = 0;
  for (let i = 0; i < n; i++) a += objs[i & 3].v;
  return a;
});
bench("ic", "poly 4", (n) => {
  const objs: any[] = [
    { v: 1 },
    { x: 0, v: 2 },
    { x: 0, y: 0, v: 3 },
    { x: 0, y: 0, z: 0, v: 4 },
  ];
  KEEP = objs;
  let a = 0;
  for (let i = 0; i < n; i++) a += objs[i & 3].v;
  return a;
});
bench("ic", "mega 8", (n) => {
  const objs: any[] = [
    { v: 1 },
    { a1: 0, v: 2 },
    { a1: 0, a2: 0, v: 3 },
    { a1: 0, a2: 0, a3: 0, v: 4 },
    { a1: 0, a2: 0, a3: 0, a4: 0, v: 5 },
    { a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, v: 6 },
    { a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, a6: 0, v: 7 },
    { a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, a6: 0, a7: 0, v: 8 },
  ];
  KEEP = objs;
  let a = 0;
  for (let i = 0; i < n; i++) a += objs[i & 7].v;
  return a;
});
bench("ic", "mega 8 write", (n) => {
  const objs: any[] = [
    { v: 1 },
    { a1: 0, v: 2 },
    { a1: 0, a2: 0, v: 3 },
    { a1: 0, a2: 0, a3: 0, v: 4 },
    { a1: 0, a2: 0, a3: 0, a4: 0, v: 5 },
    { a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, v: 6 },
    { a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, a6: 0, v: 7 },
    { a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, a6: 0, a7: 0, v: 8 },
  ];
  KEEP = objs;
  for (let i = 0; i < n; i++) objs[i & 7].v = i;
  return objs[0].v;
});
// The same question about a CALL site rather than a read site: one callee, two,
// four. A call whose callee is not the one the site remembers is the shape an
// interpreter handles for free and a compiler does not.
bench("ic", "call mono", (n) => {
  const fns = [(x: number) => x + 1, (x: number) => x + 1, (x: number) => x + 1, (x: number) => x + 1];
  KEEP = fns;
  let a = 0;
  for (let i = 0; i < n; i++) a = fns[i & 3](a) | 0;
  return a;
});
bench("ic", "call poly 4", (n) => {
  const fns = [(x: number) => x + 1, (x: number) => x + 2, (x: number) => x + 3, (x: number) => x + 4];
  KEEP = fns;
  let a = 0;
  for (let i = 0; i < n; i++) a = fns[i & 3](a) | 0;
  return a;
});
bench("ic", "method poly 4", (n) => {
  class A { m(x: number): number { return x + 1; } }
  class B { m(x: number): number { return x + 2; } }
  class C { m(x: number): number { return x + 3; } }
  class D { m(x: number): number { return x + 4; } }
  const os: any[] = [new A(), new B(), new C(), new D()];
  KEEP = os;
  let a = 0;
  for (let i = 0; i < n; i++) a = os[i & 3].m(a) | 0;
  return a;
});
// The sixteenth property. This file's header states the cliff exists and that a
// row looking absurd is worth checking against it — and until today there was
// no row at the cliff, so the check had nowhere to start. Three rows: inside
// the inline slots, at the boundary, and past it.
bench("ic", "read slot 0 of 16", (n) => {
  const o: any = { p0: 1, p1: 1, p2: 1, p3: 1, p4: 1, p5: 1, p6: 1, p7: 1,
                   p8: 1, p9: 1, p10: 1, p11: 1, p12: 1, p13: 1, p14: 1, p15: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.p0;
  return a;
});
bench("ic", "read slot 14 of 16", (n) => {
  const o: any = { p0: 1, p1: 1, p2: 1, p3: 1, p4: 1, p5: 1, p6: 1, p7: 1,
                   p8: 1, p9: 1, p10: 1, p11: 1, p12: 1, p13: 1, p14: 1, p15: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.p14;
  return a;
});
bench("ic", "read slot 15 of 16", (n) => {
  const o: any = { p0: 1, p1: 1, p2: 1, p3: 1, p4: 1, p5: 1, p6: 1, p7: 1,
                   p8: 1, p9: 1, p10: 1, p11: 1, p12: 1, p13: 1, p14: 1, p15: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.p15;
  return a;
});
bench("ic", "read slot 31 of 32", (n) => {
  const o: any = {};
  for (let k = 0; k < 32; k++) o["p" + k] = 1;
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.p31;
  return a;
});

// -------------------------------------------------- allocation, for real now
//
// `plan.md` §7.1: `alloc object literal 2` measures 1.22 ns against a 1.27 ns
// floor because the literal is DELETED — `rts ir` contains no `ObjectNew` and
// `RTS_ESCAPE_STATS` reports it replaced. That row is a correct measurement of
// escape analysis and says nothing about allocation, and it has been quoted as
// though it did.
//
// These are its escaping twins. `KEEP = o` is what stops the replacement, and
// `alloc escape floor` is that store on its own so it can be subtracted.

bench("alloc", "escape floor", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    KEEP = i;
    a += i;
  }
  return a;
});
bench("alloc", "object literal 2 escaping", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = { x: i, y: i };
    KEEP = o;
    a += o.x;
  }
  return a;
});
bench("alloc", "object literal 8 escaping", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = { a: i, b: i, c: i, d: i, e: i, f: i, g: i, h: i };
    KEEP = o;
    a += o.a;
  }
  return a;
});
bench("alloc", "object literal 16 escaping", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = { p0: i, p1: i, p2: i, p3: i, p4: i, p5: i, p6: i, p7: i,
                p8: i, p9: i, p10: i, p11: i, p12: i, p13: i, p14: i, p15: i };
    KEEP = o;
    a += o.p0;
  }
  return a;
});
bench("alloc", "array literal 4 escaping", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const xs = [i, i, i, i];
    KEEP = xs;
    a += xs[0];
  }
  return a;
});
// §L8's own experiment, quoted: "hand-flattened twin row in analytic.ts
// (`const x0=i,x1=i,x2=i,x3=i; a += x0;`) against the existing row — the gap is
// the entire ceiling, measured today."
bench("alloc", "array literal 4 flattened", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const x0 = i, x1 = i, x2 = i, x3 = i;
    a += x0 + (x1 - x1) + (x2 - x2) + (x3 - x3);
  }
  return a;
});
bench("alloc", "array 64", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const xs = new Array(64);
    KEEP = xs;
    a += xs.length;
  }
  return a;
});
bench("alloc", "Array.from 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const xs = Array.from({ length: 16 }, (_v, k) => k);
    KEEP = xs;
    a += xs.length;
  }
  return a;
}, 16);
bench("alloc", "class instance 8 fields", (n) => {
  class Wide {
    a = 1; b = 2; c = 3; d = 4; e = 5; f = 6; g = 7; h = 8;
  }
  let acc = 0;
  for (let i = 0; i < n; i++) {
    const o = new Wide();
    KEEP = o;
    acc += o.a;
  }
  return acc;
});
bench("alloc", "class instance ctor args", (n) => {
  class P {
    x: number;
    y: number;
    constructor(x: number, y: number) { this.x = x; this.y = y; }
  }
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = new P(i, i);
    KEEP = o;
    a += o.x;
  }
  return a;
});
// §7.3: `array map 16` and `filter 16` above allocate their callback INSIDE the
// loop, so ~1672 of ~3534 ns per call was `closure_new` and not `map`. These are
// the hoisted twins; the difference between each pair is the closure.
bench("alloc", "closure hoisted", (n) => {
  const c = (x: number) => x + 1;
  let a = 0;
  for (let i = 0; i < n; i++) a = c(a) | 0;
  return a;
});
bench("alloc", "closure escaping", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const c = (x: number) => x + i;
    KEEP = c;
    a = c(a) | 0;
  }
  return a;
});
bench("alloc", "Symbol()", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    KEEP = Symbol();
    a += 1;
  }
  return a;
});

// ------------------------------------------------------ properties, in depth

bench("prop", "read missing", (n) => {
  const o: any = { a: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) if (o.zz === undefined) a++;
  return a;
});
bench("prop", "read proto depth 1", (n) => {
  class A { get v(): number { return 1; } }
  const o = new A();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.v;
  return a;
});
bench("prop", "read proto depth 3", (n) => {
  class A { p = 1; }
  class B extends A {}
  class C extends B {}
  class D extends C { m(): number { return 1; } }
  const o = new D();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.m();
  return a;
});
bench("prop", "getter", (n) => {
  const o = { _v: 1, get v(): number { return this._v; } };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.v;
  return a;
});
bench("prop", "setter", (n) => {
  const o = { _v: 1, set v(x: number) { this._v = x; } };
  KEEP = o;
  for (let i = 0; i < n; i++) o.v = i;
  return o._v;
});
bench("prop", "hasOwnProperty", (n) => {
  const o: any = { a: 1, b: 2 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) if (Object.prototype.hasOwnProperty.call(o, "a")) a++;
  return a;
});
bench("prop", "symbol key read", (n) => {
  const k = Symbol("k");
  const o: any = {};
  o[k] = 1;
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o[k];
  return a;
});
bench("prop", "numeric key read", (n) => {
  const o: any = { 0: 1, 1: 2, 2: 3, 3: 4 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o[i & 3];
  return a;
});
bench("prop", "frozen read", (n) => {
  const o = Object.freeze({ a: 1 });
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.a;
  return a;
});
bench("prop", "delete+add", (n) => {
  const o: any = { a: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) {
    delete o.a;
    o.a = i;
    a += 1;
  }
  return a;
}, 2);
bench("prop", "spread 4", (n) => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) {
    const c = { ...o };
    KEEP = c;
    a += c.a;
  }
  return a;
}, 4);
bench("prop", "Object.assign 4", (n) => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  let a = 0;
  for (let i = 0; i < n; i++) {
    const c = Object.assign({}, o);
    KEEP = c;
    a += c.a;
  }
  return a;
}, 4);
bench("prop", "destructure 4", (n) => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  KEEP = o;
  let acc = 0;
  for (let i = 0; i < n; i++) {
    const { a, b, c, d } = o;
    acc += a + b + c + d;
  }
  return acc;
}, 4);
bench("prop", "Proxy get", (n) => {
  const p: any = new Proxy({ a: 1 }, { get: (t: any, k: any) => t[k] });
  KEEP = p;
  let a = 0;
  for (let i = 0; i < n; i++) a += p.a;
  return a;
});
// `plan.md` §S3 calls this "the best experiment in this dataset, and it needs no
// engine change at all". A text cell has no shape and no recorded prototype, so
// `cache_resolve_indirect` REFUSES the site and the resolver runs on every
// execution, forever. A wrapper is an ordinary cell with both, so its site can
// arm — and the native body on the other end is byte-identical, because both go
// through the same `coerce_receiver`. The difference between these two rows is
// the whole of the ~68 ns tax under every `String.prototype` method.
bench("prop", "method on primitive string", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.toUpperCase().length;
  return a;
});
bench("prop", "method on String wrapper", (n) => {
  const w = new String("abcdefghijklmnop");
  KEEP = w;
  let a = 0;
  for (let i = 0; i < n; i++) a += w.toUpperCase().length;
  return a;
});
// §7.6: `prop typeof alone` above reads a MODULE-level const out of the
// environment object, so its number carries a property read and two throw
// checks that nothing in the name suggests. This is the same operator on a
// local, which is the number a per-crossing cost should be derived from.
bench("prop", "typeof local", (n) => {
  const o = { a: 1 };
  let a = 0;
  for (let i = 0; i < n; i++) if (typeof o) a++;
  return a;
});

// -------------------------------------------------------------- classes, new

bench("class", "field read", (n) => {
  class P { x = 1; }
  const o = new P();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.x;
  return a;
});
bench("class", "private field read", (n) => {
  class P {
    #x = 1;
    get(): number { return this.#x; }
  }
  const o = new P();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.get();
  return a;
});
bench("class", "static method", (n) => {
  class P { static m(x: number): number { return x + 1; } }
  KEEP = P;
  let a = 0;
  for (let i = 0; i < n; i++) a = P.m(a) | 0;
  return a;
});
bench("class", "super method call", (n) => {
  class A { m(x: number): number { return x + 1; } }
  class B extends A { m(x: number): number { return super.m(x); } }
  const o = new B();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a = o.m(a) | 0;
  return a;
});
bench("class", "accessor pair", (n) => {
  class P {
    _v = 0;
    get v(): number { return this._v; }
    set v(x: number) { this._v = x; }
  }
  const o = new P();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) {
    o.v = i;
    a += o.v;
  }
  return a | 0;
}, 2);
bench("class", "instanceof depth 3", (n) => {
  class A {}
  class B extends A {}
  class C extends B {}
  class D extends C {}
  const o = new D();
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) if (o instanceof A) a++;
  return a;
});

// ------------------------------------------------------------ calls, in depth
//
// `plan.md` §7.2: `call free function` and `call arrow` above CONTAIN NO CALL —
// `emit/inline.rs` substitutes the body and the loop is one `FloatArith`. Any
// per-call cost derived by differencing against them is invalid. These rows are
// shapes inlining cannot take, so the pair says what inlining is worth.

bench("call", "not inlinable (recursive)", (n) => {
  function rec(x: number, d: number): number {
    return d === 0 ? x + 1 : rec(x, d - 1);
  }
  KEEP = rec;
  let a = 0;
  for (let i = 0; i < n; i++) a = rec(a, 0) | 0;
  return a;
});
bench("call", "through a variable", (n) => {
  let f = (x: number) => x + 1;
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a = f(a) | 0;
  return a;
});
bench("call", "as an argument", (n) => {
  function apply(g: (x: number) => number, x: number): number { return g(x); }
  const inc = (x: number) => x + 1;
  KEEP = inc;
  let a = 0;
  for (let i = 0; i < n; i++) a = apply(inc, a) | 0;
  return a;
});
bench("call", "0 args", (n) => {
  function f(): number { return 1; }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a += f();
  return a;
});
bench("call", "4 args", (n) => {
  function f(a1: number, a2: number, a3: number, a4: number): number {
    return a1 + a2 + a3 + a4;
  }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a += f(1, 2, 3, 4);
  return a | 0;
});
bench("call", "8 args", (n) => {
  function f(a1: number, a2: number, a3: number, a4: number,
             a5: number, a6: number, a7: number, a8: number): number {
    return a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8;
  }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a += f(1, 2, 3, 4, 5, 6, 7, 8);
  return a | 0;
});
bench("call", "default params", (n) => {
  function f(x: number, y: number = 1): number { return x + y; }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a = f(a) | 0;
  return a;
});
bench("call", "destructured param", (n) => {
  function f({ x, y }: { x: number; y: number }): number { return x + y; }
  const arg = { x: 1, y: 1 };
  KEEP = arg;
  let a = 0;
  for (let i = 0; i < n; i++) a += f(arg);
  return a | 0;
});
bench("call", "spread call 3", (n) => {
  function f(a1: number, a2: number, a3: number): number { return a1 + a2 + a3; }
  const xs = [1, 2, 3];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += f(...xs);
  return a | 0;
});
bench("call", ".call()", (n) => {
  function f(this: any, x: number): number { return x + 1; }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a = f.call(null, a) | 0;
  return a;
});
bench("call", ".apply()", (n) => {
  function f(this: any, x: number): number { return x + 1; }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a = f.apply(null, [a]) | 0;
  return a;
});
bench("call", "bound function", (n) => {
  function f(this: any, x: number): number { return x + 1; }
  const b = f.bind(null);
  KEEP = b;
  let a = 0;
  for (let i = 0; i < n; i++) a = b(a) | 0;
  return a;
});
bench("call", "recursion depth 8", (n) => {
  function rec(d: number): number { return d === 0 ? 0 : 1 + rec(d - 1); }
  KEEP = rec;
  let a = 0;
  for (let i = 0; i < n; i++) a += rec(8);
  return a | 0;
}, 8);
bench("call", "arguments object", (n) => {
  function f(): number {
    // eslint-disable-next-line prefer-rest-params
    return arguments.length;
  }
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a += (f as any)(1, 2, 3);
  return a;
});
bench("call", "optional call", (n) => {
  const f: ((x: number) => number) | null = (x: number) => x + 1;
  KEEP = f;
  let a = 0;
  for (let i = 0; i < n; i++) a = (f?.(a) ?? 0) | 0;
  return a;
});
bench("call", "closure capture depth 3", (n) => {
  const outer = 1;
  const mid = () => {
    const m = outer + 1;
    return () => m + 1;
  };
  const inner = mid();
  KEEP = inner;
  let a = 0;
  for (let i = 0; i < n; i++) a += inner();
  return a;
});

// ------------------------------------------------------------ arrays, wider

bench("array", "index read float", (n) => {
  const xs: number[] = [];
  for (let k = 0; k < 1024; k++) xs.push(k + 0.5);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs[i & 1023];
  return a | 0;
});
bench("array", "index read mixed", (n) => {
  const xs: any[] = [];
  for (let k = 0; k < 1024; k++) xs.push(k % 3 === 0 ? "s" : k);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) {
    const v = xs[i & 1023];
    a += typeof v === "number" ? v : 1;
  }
  return a | 0;
});
bench("array", "index read out of range", (n) => {
  const xs = [1, 2, 3, 4];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) if (xs[9] === undefined) a++;
  return a;
});
bench("array", "length read", (n) => {
  const xs = [1, 2, 3, 4];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.length;
  return a;
});
bench("array", "push 16 fresh", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const xs: number[] = [];
    for (let k = 0; k < 16; k++) xs.push(k);
    KEEP = xs;
    a += xs.length;
  }
  return a;
}, 16);
bench("array", "classic for 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) for (let k = 0; k < xs.length; k++) a += xs[k];
  return a | 0;
}, 16);
bench("array", "forEach 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const cb = (x: number) => { a += x; };
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) xs.forEach(cb);
  return a | 0;
}, 16);
bench("array", "map 16 hoisted cb", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const cb = (x: number) => x + 1;
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.map(cb)[0];
  return a;
}, 16);
bench("array", "filter 16 hoisted cb", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const cb = (x: number) => x > 8;
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.filter(cb).length;
  return a;
}, 16);
bench("array", "reduce 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const cb = (p: number, x: number) => p + x;
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.reduce(cb, 0);
  return a | 0;
}, 16);
bench("array", "some 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const cb = (x: number) => x === 15;
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) if (xs.some(cb)) a++;
  return a;
}, 16);
bench("array", "find 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  const cb = (x: number) => x === 15;
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.find(cb) as number;
  return a;
}, 16);
bench("array", "includes 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) if (xs.includes(15)) a++;
  return a;
}, 16);
bench("array", "slice 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.slice(0, 16).length;
  return a;
}, 16);
bench("array", "concat 8+8", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7];
  const ys = [8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.concat(ys).length;
  return a;
}, 16);
bench("array", "spread copy 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) {
    const c = [...xs];
    KEEP = c;
    a += c.length;
  }
  return a;
}, 16);
bench("array", "sort 16 (incl. copy)", (n) => {
  const xs = [9, 3, 15, 1, 7, 11, 5, 13, 0, 8, 2, 14, 6, 10, 4, 12];
  const cmp = (p: number, q: number) => p - q;
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.slice().sort(cmp)[0];
  return a;
}, 16);
bench("array", "reverse 16 (incl. copy)", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.slice().reverse()[0];
  return a;
}, 16);
bench("array", "shift+unshift", (n) => {
  const xs = [1, 2, 3, 4];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) {
    xs.unshift(i);
    a += xs.shift() as number;
  }
  return a | 0;
}, 2);
bench("array", "destructure [a,b]", (n) => {
  const xs = [1, 2, 3, 4];
  KEEP = xs;
  let acc = 0;
  for (let i = 0; i < n; i++) {
    const [a, b] = xs;
    acc += a + b;
  }
  return acc | 0;
}, 2);
bench("array", "flat 4x4", (n) => {
  const xs = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12], [13, 14, 15, 16]];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.flat().length;
  return a;
}, 16);
bench("array", "2D read", (n) => {
  const g: number[][] = [];
  for (let r = 0; r < 32; r++) {
    const row: number[] = [];
    for (let c = 0; c < 32; c++) row.push(r * c);
    g.push(row);
  }
  KEEP = g;
  let a = 0;
  for (let i = 0; i < n; i++) a += g[i & 31][(i >> 5) & 31];
  return a | 0;
});
bench("array", "Array.isArray", (n) => {
  const xs = [1, 2, 3, 4];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) if (Array.isArray(xs)) a++;
  return a;
});

// ----------------------------------------------------------- strings, wider

bench("string", "startsWith", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) if (s.startsWith("abc")) a++;
  return a;
});
bench("string", "endsWith", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) if (s.endsWith("nop")) a++;
  return a;
});
bench("string", "includes", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) if (s.includes("hij")) a++;
  return a;
});
bench("string", "replace literal", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.replace("hij", "X").length;
  return a;
});
bench("string", "trim", (n) => {
  const s = "  abcdefghijklmnop  ";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.trim().length;
  return a;
});
bench("string", "padStart 24", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.padStart(24, "0").length;
  return a;
});
bench("string", "repeat 4", (n) => {
  const s = "abcd";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.repeat(4).length;
  return a;
}, 4);
bench("string", "codePointAt", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.codePointAt(i & 15) as number;
  return a;
});
bench("string", "fromCharCode", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += String.fromCharCode(65 + (i & 15)).length;
  return a;
});
bench("string", "compare <", (n) => {
  const s = "abcdefghijklmnop";
  const t = "abcdefghijklmnoq";
  let a = 0;
  for (let i = 0; i < n; i++) if (s < t) a++;
  return a;
});
bench("string", "Number(str)", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Number("12345");
  return a;
});
bench("string", "toFixed 2", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (1.23456).toFixed(2).length;
  return a;
});
bench("string", "toString(16)", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (i & 65535).toString(16).length;
  return a;
});
// §L4's own experiment, quoted: three rows whose SLOPE is the per-piece concat
// and whose intercept is the fixed machinery. `text::template_join` allocates
// about nine times and two region cells for a five-character answer, under a
// comment claiming "one buffer, grown once" — which the plan records as false.
bench("string", "template 1 hole", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += `${i}`.length;
  return a;
});
bench("string", "template 2 holes", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += `v=${i}!${i}?`.length;
  return a;
});
bench("string", "template 4 holes", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += `${i}a${i}b${i}c${i}`.length;
  return a;
});
bench("string", "concat 4 pieces", (n) => {
  const s = "abcd";
  let a = 0;
  for (let i = 0; i < n; i++) a += (s + "-" + s + "-" + s).length;
  return a;
}, 4);
bench("string", "build 16 by +=", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    let s = "";
    for (let k = 0; k < 16; k++) s += "x";
    a += s.length;
  }
  return a;
}, 16);
bench("string", "join 16 pieces", (n) => {
  const parts = ["a", "b", "c", "d", "e", "f", "g", "h",
                 "i", "j", "k", "l", "m", "n", "o", "p"];
  KEEP = parts;
  let a = 0;
  for (let i = 0; i < n; i++) a += parts.join("").length;
  return a;
}, 16);
bench("string", "for-of chars 16", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) for (const ch of s) a += ch.length;
  return a;
}, 16);
bench("string", "localeCompare", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.localeCompare("abcdefghijklmnoq");
  return a | 0;
});
bench("string", "normalize NFC", (n) => {
  const s = "abcdefghijklmnop";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.normalize("NFC").length;
  return a;
});

// ---------------------------------------------------------- collections, wider

bench("coll", "Map.get miss", (n) => {
  const m = new Map<string, number>();
  for (let k = 0; k < 64; k++) m.set("k" + k, k);
  KEEP = m;
  let a = 0;
  for (let i = 0; i < n; i++) if (m.get("zz") === undefined) a++;
  return a;
});
bench("coll", "Map.get numeric key", (n) => {
  const m = new Map<number, number>();
  for (let k = 0; k < 64; k++) m.set(k, k);
  KEEP = m;
  let a = 0;
  for (let i = 0; i < n; i++) a += m.get(i & 63) as number;
  return a;
});
bench("coll", "Map.get object key", (n) => {
  const key = { id: 1 };
  const m = new Map<any, number>();
  m.set(key, 7);
  KEEP = m;
  let a = 0;
  for (let i = 0; i < n; i++) a += m.get(key) as number;
  return a;
});
bench("coll", "Map build 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const m = new Map<number, number>();
    for (let k = 0; k < 16; k++) m.set(k, k);
    KEEP = m;
    a += m.size;
  }
  return a;
}, 16);
bench("coll", "Map delete+set", (n) => {
  const m = new Map<number, number>();
  for (let k = 0; k < 64; k++) m.set(k, k);
  KEEP = m;
  let a = 0;
  for (let i = 0; i < n; i++) {
    m.delete(7);
    m.set(7, i);
    a += 1;
  }
  return a;
}, 2);
bench("coll", "Map for-of 16", (n) => {
  const m = new Map<number, number>();
  for (let k = 0; k < 16; k++) m.set(k, k);
  KEEP = m;
  let a = 0;
  for (let i = 0; i < n; i++) for (const [, v] of m) a += v;
  return a | 0;
}, 16);
bench("coll", "Set build 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const s = new Set<number>();
    for (let k = 0; k < 16; k++) s.add(k);
    KEEP = s;
    a += s.size;
  }
  return a;
}, 16);
bench("coll", "Set for-of 16", (n) => {
  const s = new Set<number>();
  for (let k = 0; k < 16; k++) s.add(k);
  KEEP = s;
  let a = 0;
  for (let i = 0; i < n; i++) for (const v of s) a += v;
  return a | 0;
}, 16);
bench("coll", "WeakMap get", (n) => {
  const key = { id: 1 };
  const w = new WeakMap<any, number>();
  w.set(key, 7);
  KEEP = w;
  let a = 0;
  for (let i = 0; i < n; i++) a += w.get(key) as number;
  return a;
});
bench("coll", "Object.values 4", (n) => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += Object.values(o).length;
  return a;
}, 4);
bench("coll", "Object.entries 4", (n) => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += Object.entries(o).length;
  return a;
}, 4);
bench("coll", "for-in 4", (n) => {
  const o: any = { a: 1, b: 2, c: 3, d: 4 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) for (const k in o) a += o[k];
  return a | 0;
}, 4);

// ------------------------------------------------------------- json, wider

bench("json", "stringify array 100", (n) => {
  const xs: number[] = [];
  for (let k = 0; k < 100; k++) xs.push(k);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += JSON.stringify(xs).length;
  return a;
}, 100);
bench("json", "parse array 100", (n) => {
  const xs: number[] = [];
  for (let k = 0; k < 100; k++) xs.push(k);
  const text = JSON.stringify(xs);
  KEEP = text;
  let a = 0;
  for (let i = 0; i < n; i++) a += (JSON.parse(text) as number[]).length;
  return a;
}, 100);
bench("json", "stringify nested 3", (n) => {
  const o = { a: { b: { c: [1, 2, 3], d: "x" }, e: 1 }, f: true };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += JSON.stringify(o).length;
  return a;
});
bench("json", "parse nested 3", (n) => {
  const text = JSON.stringify({ a: { b: { c: [1, 2, 3], d: "x" }, e: 1 }, f: true });
  KEEP = text;
  let a = 0;
  for (let i = 0; i < n; i++) a += (JSON.parse(text) as any).f ? 1 : 0;
  return a;
});
bench("json", "stringify string 256", (n) => {
  const s = "abcdefghijklmnop".repeat(16);
  KEEP = s;
  let a = 0;
  for (let i = 0; i < n; i++) a += JSON.stringify(s).length;
  return a;
});

// ------------------------------------------------------------- regex, wider
//
// `compile.rs` already records the shape this pair tests: "280 ns for a
// three-character subject, of which only 85 more appear when the subject grows
// to 251 — so the cost was per CALL and not per character." Two rows, so a
// reader can check that rather than take it.

bench("regex", "test short subject", (n) => {
  const r = /[a-f]+([0-9]+)/;
  const s = "abc123";
  let a = 0;
  for (let i = 0; i < n; i++) if (r.test(s)) a++;
  return a;
});
bench("regex", "test long subject", (n) => {
  const r = /[a-f]+([0-9]+)/;
  const s = "abcdefghijklmnop".repeat(16) + "abc123";
  let a = 0;
  for (let i = 0; i < n; i++) if (r.test(s)) a++;
  return a;
});
bench("regex", "test no match", (n) => {
  const r = /[a-f]+([0-9]+)/;
  const s = "zzzzzzzzzzzzzzzz";
  let a = 0;
  for (let i = 0; i < n; i++) if (!r.test(s)) a++;
  return a;
});
// A NAMED group. `plan.md` §S2 records that `regex/mod.rs` calls
// `well_known(name)` with the user's own capture-group name — a name that
// cannot be on the cached list, so it is allocated and hashed once per group
// per match, forever. This row against `exec+group` above is that cost.
bench("regex", "exec named group", (n) => {
  const r = /[a-f]+(?<num>[0-9]+)/;
  const s = "abc123";
  let a = 0;
  for (let i = 0; i < n; i++) {
    const m = r.exec(s);
    a += m === null ? 0 : (m.groups as any).num.length;
  }
  return a;
});
bench("regex", "matchAll 4", (n) => {
  const s = "a1 b2 c3 d4";
  let a = 0;
  for (let i = 0; i < n; i++) {
    const r = /([a-z])([0-9])/g;
    for (const m of s.matchAll(r)) a += m[1].length;
  }
  return a;
}, 4);
bench("regex", "split by regex", (n) => {
  const s = "a1b2c3d4e5f6g7h8";
  let a = 0;
  for (let i = 0; i < n; i++) a += s.split(/[0-9]/).length;
  return a;
}, 8);
bench("regex", "replace with fn", (n) => {
  const r = /[a-f]+([0-9]+)/;
  const s = "abc123";
  const fn = (_m: string, g: string) => g;
  let a = 0;
  for (let i = 0; i < n; i++) a += s.replace(r, fn).length;
  return a;
});
bench("regex", "new RegExp per call", (n) => {
  const s = "abc123";
  let a = 0;
  for (let i = 0; i < n; i++) {
    const r = new RegExp("[a-f]+([0-9]+)");
    KEEP = r;
    if (r.test(s)) a++;
  }
  return a;
});

// ------------------------------------------------------------ binary, wider

bench("binary", "Int32Array rw", (n) => {
  const xs = new Int32Array(256);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) {
    xs[i & 255] = i;
    a += xs[i & 255];
  }
  return a | 0;
}, 2);
bench("binary", "Float32Array rw", (n) => {
  const xs = new Float32Array(256);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) {
    xs[i & 255] = i;
    a += xs[i & 255];
  }
  return a | 0;
}, 2);
bench("binary", "DataView setF64", (n) => {
  const dv2 = new DataView(new ArrayBuffer(64));
  KEEP = dv2;
  for (let i = 0; i < n; i++) dv2.setFloat64(0, i, true);
  return dv2.getFloat64(0, true) | 0;
});
bench("binary", "typed set 64", (n) => {
  const dst = new Uint8Array(1024);
  const src = new Uint8Array(64);
  KEEP = dst;
  for (let i = 0; i < n; i++) dst.set(src, 0);
  return dst[0];
}, 64);
bench("binary", "typed fill 64", (n) => {
  const xs = new Uint8Array(64);
  KEEP = xs;
  for (let i = 0; i < n; i++) xs.fill(i & 255);
  return xs[0];
}, 64);
bench("binary", "typed for-of 16", (n) => {
  const xs = new Uint8Array(16);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) for (const v of xs) a += v;
  return a | 0;
}, 16);
bench("binary", "TextDecoder 16", (n) => {
  const dec = new TextDecoder();
  const bytes = new Uint8Array(16);
  KEEP = dec;
  let a = 0;
  for (let i = 0; i < n; i++) a += dec.decode(bytes).length;
  return a;
});
bench("binary", "alloc ArrayBuffer 64", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const b = new ArrayBuffer(64);
    KEEP = b;
    a += b.byteLength;
  }
  return a;
});
bench("binary", "byteLength read", (n) => {
  const xs = new Uint8Array(64);
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) a += xs.byteLength;
  return a;
});

// --------------------------------------------------------- control flow, wider
//
// There is deliberately NO async row here. Every case is `run(n): number` and
// the harness times it synchronously, so the only thing an `await`-free promise
// row could do is queue `n` microtasks that drain after the clock stops —
// measuring the queue's growth and reporting it as the operation. A promise
// instrument is a different harness, not a row in this one.

bench("flow", "while loop", (n) => {
  let a = 0;
  let i = 0;
  while (i < n) { a += i; i++; }
  return a;
});
bench("flow", "do-while loop", (n) => {
  let a = 0;
  let i = 0;
  do { a += i; i++; } while (i < n);
  return a;
});
bench("flow", "nested loop 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) for (let k = 0; k < 16; k++) a += k;
  return a;
}, 16);
bench("flow", "labeled break 16", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    outer: for (let k = 0; k < 16; k++) {
      for (let j = 0; j < 16; j++) if (j === 8) { a += 1; break outer; }
    }
  }
  return a;
});
bench("flow", "try/finally no throw", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    try { a += 1; } finally { a += 0; }
  }
  return a;
});
bench("flow", "throw across a call", (n) => {
  function boom(): number { throw new Error("x"); }
  KEEP = boom;
  let a = 0;
  for (let i = 0; i < n; i++) {
    try { boom(); } catch { a += 1; }
  }
  return a;
});
bench("flow", "conditional ?:", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += (i & 1) === 0 ? 1 : 2;
  return a;
});
bench("flow", "logical &&", (n) => {
  const o: any = { a: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += (o && o.a) as number;
  return a;
});
bench("flow", "nullish ??", (n) => {
  const o: any = { a: 1 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += o.zz ?? 1;
  return a;
});
bench("flow", "switch on string", (n) => {
  const keys = ["a", "b", "c", "d", "e", "f", "g", "h"];
  KEEP = keys;
  let a = 0;
  for (let i = 0; i < n; i++) {
    switch (keys[i & 7]) {
      case "a": a += 1; break;
      case "b": a += 2; break;
      case "c": a += 3; break;
      case "d": a += 4; break;
      case "e": a += 5; break;
      case "f": a += 6; break;
      case "g": a += 7; break;
      default: a += 8; break;
    }
  }
  return a;
});
bench("flow", "generator for-of 16", (n) => {
  function* g(): Generator<number> {
    for (let k = 0; k < 16; k++) yield k;
  }
  KEEP = g;
  let a = 0;
  for (let i = 0; i < n; i++) for (const v of g()) a += v;
  return a | 0;
}, 16);
bench("flow", "manual iterator 16", (n) => {
  const xs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
  KEEP = xs;
  let a = 0;
  for (let i = 0; i < n; i++) {
    const it = xs[Symbol.iterator]();
    for (;;) {
      const r = it.next();
      if (r.done === true) break;
      a += r.value;
    }
  }
  return a | 0;
}, 16);

// ----------------------------------------------------------------- date, misc

bench("misc", "Date.now", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += Date.now() & 1;
  return a;
});
bench("misc", "performance.now", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) a += performance.now() & 1;
  return a;
});
bench("misc", "new Date()", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const d = new Date(0);
    KEEP = d;
    a += 1;
  }
  return a;
});
bench("misc", "date.getTime", (n) => {
  const d = new Date(0);
  KEEP = d;
  let a = 0;
  for (let i = 0; i < n; i++) a += d.getTime() & 1;
  return a;
});
bench("misc", "date.toISOString", (n) => {
  const d = new Date(0);
  KEEP = d;
  let a = 0;
  for (let i = 0; i < n; i++) a += d.toISOString().length;
  return a;
});
bench("misc", "Object.freeze 4", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) {
    const o = Object.freeze({ a: i, b: i, c: i, d: i });
    KEEP = o;
    a += o.a;
  }
  return a;
}, 4);
bench("misc", "String(obj)", (n) => {
  const o = { toString(): string { return "x"; } };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) a += String(o).length;
  return a;
});
bench("misc", "Number.isInteger", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (Number.isInteger(i)) a++;
  return a;
});
bench("misc", "isNaN", (n) => {
  let a = 0;
  for (let i = 0; i < n; i++) if (!isNaN(i)) a++;
  return a;
});
bench("misc", "BigInt add", (n) => {
  let a = 0n;
  for (let i = 0; i < n; i++) a = a + 3n;
  return Number(a & 1023n);
});
bench("misc", "structuredClone 4", (n) => {
  const o = { a: 1, b: 2, c: 3, d: 4 };
  KEEP = o;
  let a = 0;
  for (let i = 0; i < n; i++) {
    const c = structuredClone(o);
    KEEP = c;
    a += c.a;
  }
  return a;
}, 4);

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

type Row = { group: string; name: string; ops: number; nanos: number; failed: string };

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
    return { group: c.group, name: c.name, ops: c.ops, nanos: (best * 1e6) / (n * c.ops), failed: "" };
  } catch (e) {
    // A case that throws is DATA, not an interruption: an action this engine
    // does not have is exactly what an analytic of what it costs should report,
    // and stopping at the first one would report nothing about anything after.
    return { group: c.group, name: c.name, ops: c.ops, nanos: 0, failed: String(e).slice(0, 60) };
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
// The `ops` column is printed and was not, and `plan.md` §7.5 is why: the
// divisor is not uniform — 16 for the sixteen-element rows, 4 for the four-key
// ones, 2 for a pair — and three separate verdicts read a row as if it were 1.
// A divisor a reader has to go and look up in the source is a divisor a reader
// gets wrong.
console.log("action                                    ops     ns/op    minus floor");
console.log("---------------------------------------------------------------------");
for (const r of rows) {
  const label = pad(r.group + " " + r.name, 38);
  if (r.failed !== "") {
    console.log(label + "  UNAVAILABLE  " + r.failed);
    continue;
  }
  const net = r.nanos - floor;
  console.log(
    label +
      padLeft(String(r.ops), 4) +
      padLeft(r.nanos.toFixed(2), 10) +
      padLeft(net > 0 ? net.toFixed(2) : "~0", 13),
  );
}
console.log("---------------------------------------------------------------------");
console.log("floor (empty loop iteration): " + floor.toFixed(2) + " ns");
console.log(rows.length + " cases, " + rows.filter((r) => r.failed !== "").length + " unavailable");
console.log("checksum " + SINK + " " + (KEEP === null ? "-" : "+"));
