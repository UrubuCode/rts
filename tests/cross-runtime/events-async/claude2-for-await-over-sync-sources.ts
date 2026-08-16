// Cross-runtime: `for await` over a SYNC iterable awaits every value it pulls --
// plain values, promises and thenables alike -- and the pull log shows it pulls
// one at a time rather than materialising the sequence.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

function makeIterable(values: any[], tag: string) {
  return {
    [Symbol.iterator]: function () {
      let i = 0;
      return {
        next: function () {
          pulls.push(tag + "-next" + i);
          if (i >= values.length) return { done: true, value: undefined };
          return { done: false, value: values[i++] };
        },
        return: function (v: any) { pulls.push(tag + "-return"); return { done: true, value: v }; }
      };
    }
  };
}

(async function () {
  // 1) plain values
  pulls.length = 0;
  const got1: string[] = [];
  for await (const v of makeIterable([1, 2, 3], "a") as any) got1.push(String(v));
  log("plainValues=" + got1.join(",") + " pulls=" + pulls.join(","));

  // 2) promises are UNWRAPPED by for-await
  pulls.length = 0;
  const got2: string[] = [];
  for await (const v of makeIterable([Promise.resolve("p1"), Promise.resolve("p2")], "b") as any) got2.push(String(v));
  log("promises=" + got2.join(",") + " pulls=" + pulls.join(","));

  // 3) so are thenables
  pulls.length = 0;
  const got3: string[] = [];
  const th = function (v: string) { return { then: function (r: any) { r(v); } }; };
  for await (const v of makeIterable([th("t1"), th("t2")], "c") as any) got3.push(String(v));
  log("thenables=" + got3.join(",") + " pulls=" + pulls.join(","));

  // 4) interleaving: a competing microtask ladder shows the loop yields between
  //    iterations rather than running straight through
  const marks: string[] = [];
  let rung: Promise<void> = Promise.resolve();
  for (let i = 1; i <= 8; i++) {
    const k = i;
    rung = rung.then(function () { marks.push("r" + k); });
  }
  for await (const v of makeIterable(["x", "y", "z"], "d") as any) marks.push("it" + v);
  log("interleaved=" + marks.join(","));

  // 5) a rejected promise in the sequence throws INTO the loop and stops it.
  //    Whether the sync iterator is CLOSED on that path is not asserted: Bun
  //    calls its `return`, Node does not, so only the values and the reason are
  //    comparable.
  pulls.length = 0;
  const got5: string[] = [];
  let caught5 = "none";
  try {
    for await (const v of makeIterable(["ok", Promise.reject("BOOM"), "never"], "e") as any) got5.push(String(v));
  } catch (e: any) { caught5 = String(e); }
  log("rejectedEntry=" + got5.join(",") + " caught=" + caught5 +
    " pullsBeforeReturn=" + pulls.filter(function (p) { return p.indexOf("return") < 0; }).join(","));

  // 6) `break` closes the sync iterator through the async wrapper
  pulls.length = 0;
  const got6: string[] = [];
  for await (const v of makeIterable([1, 2, 3, 4], "f") as any) {
    got6.push(String(v));
    if (v === 2) break;
  }
  log("break=" + got6.join(",") + " pulls=" + pulls.join(","));

  // 7) for-await over a plain ARRAY of promises does the same
  const got7: string[] = [];
  for await (const v of [Promise.resolve("A"), "B", Promise.resolve("C")] as any) got7.push(String(v));
  log("arrayOfPromises=" + got7.join(","));

  // 8) an object with BOTH Symbol.asyncIterator and Symbol.iterator prefers the
  //    async one
  const both: any = {
    [Symbol.iterator]: function () { return { next: function () { return { done: true, value: undefined }; } }; },
    [Symbol.asyncIterator]: function () {
      let i = 0;
      return { next: function () { i++; return Promise.resolve(i <= 2 ? { done: false, value: "async" + i } : { done: true, value: undefined }); } };
    }
  };
  const got8: string[] = [];
  for await (const v of both) got8.push(String(v));
  log("prefersAsyncIterator=" + got8.join(","));

  // 9) an async generator is the ordinary source, and its values are already
  //    awaited for you
  async function* agen() { yield "g1"; yield Promise.resolve("g2"); yield "g3"; }
  const got9: string[] = [];
  for await (const v of agen()) got9.push(String(v));
  log("asyncGenerator=" + got9.join(","));

  // 10) for-await over an EMPTY source runs the body zero times but still
  //     costs at least one pull
  pulls.length = 0;
  let bodyRuns = 0;
  for await (const v of makeIterable([], "g") as any) bodyRuns++;
  log("empty=" + bodyRuns + " pulls=" + pulls.join(","));

  // 11) built-in iterables work unchanged: a Map, a Set and a string
  const got11: string[] = [];
  for await (const entry of new Map([["k1", "v1"], ["k2", "v2"]])) got11.push(entry[0] + "=" + entry[1]);
  log("map=" + got11.join(","));
  const got11b: string[] = [];
  for await (const v of new Set(["s1", "s2"])) got11b.push(String(v));
  log("set=" + got11b.join(","));
  const got11c: string[] = [];
  for await (const ch of "abc") got11c.push(ch);
  log("string=" + got11c.join(","));

  // 12) an async iterator whose next() answers a PLAIN result object, not a
  //     promise, is accepted
  const syncResults: any = {
    [Symbol.asyncIterator]: function () {
      let i = 0;
      return { next: function () { i++; return i <= 2 ? { done: false, value: "r" + i } : { done: true, value: undefined }; } };
    }
  };
  const got12: string[] = [];
  for await (const v of syncResults) got12.push(String(v));
  log("plainResultObjects=" + got12.join(","));

  // 13) `continue` does not close the source; the loop keeps pulling
  pulls.length = 0;
  const got13: string[] = [];
  for await (const v of makeIterable([1, 2, 3, 4], "h") as any) {
    if ((v as any) % 2 === 0) continue;
    got13.push(String(v));
  }
  log("continue=" + got13.join(",") + " pulls=" + pulls.join(","));

  // 14) a `return` out of the loop closes the source too
  pulls.length = 0;
  const returned = await (async function () {
    for await (const v of makeIterable([1, 2, 3], "i") as any) { if ((v as any) === 2) return "left-at-" + v; }
    return "drained";
  })();
  log("returnFromLoop=" + returned + " pulls=" + pulls.join(","));

  // 15) a source that is not iterable at all is a TypeError
  let notIterable = "no";
  try { for await (const v of ({} as any)) { notIterable = "iterated"; } }
  catch (e: any) { notIterable = e.constructor.name; }
  log("notIterable=" + notIterable);

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
