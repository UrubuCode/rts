// Cross-runtime: an async generator QUEUES its requests. Several next() calls
// made before any of them settles are served in call order, and a return()
// queued behind them waits its turn -- it does not jump the queue.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }
function step(r: any): string { return String(r.value) + ":" + r.done; }

const trace: string[] = [];

async function* source(tag: string) {
  try {
    let i = 0;
    while (i < 5) {
      trace.push(tag + ":before" + i);
      await Promise.resolve();
      yield tag + i;
      i++;
    }
    return tag + "-exhausted";
  } finally {
    trace.push(tag + ":closed");
  }
}

(async function () {
  // 1) three next() calls issued back-to-back are served in order
  trace.length = 0;
  const a = source("a");
  const pa1 = a.next();
  const pa2 = a.next();
  const pa3 = a.next();
  log("1 allPromises=" + (pa1 instanceof Promise) + (pa2 instanceof Promise) + (pa3 instanceof Promise));
  log("1 distinct=" + (pa1 !== pa2 && pa2 !== pa3));
  const rs = await Promise.all([pa1, pa2, pa3]);
  log("1 results=" + rs.map(step).join(" "));
  log("1 trace=" + trace.join("|"));

  // 2) a return() queued behind two next() calls settles last, and the two
  //    next() calls still get their values
  trace.length = 0;
  const b = source("b");
  const pb1 = b.next();
  const pb2 = b.next();
  const pbr = b.return("stopped" as any);
  const pb3 = b.next();
  log("2 first=" + step(await pb1));
  log("2 second=" + step(await pb2));
  log("2 return=" + step(await pbr));
  log("2 afterReturn=" + step(await pb3));
  log("2 trace=" + trace.join("|"));

  // 3) throw() queued behind a next() reaches the generator at its turn
  trace.length = 0;
  async function* guarded() {
    try {
      yield "g1";
      yield "g2";
    } catch (e: any) {
      trace.push("caught " + e.constructor.name);
      yield "recovered";
    } finally {
      trace.push("guarded:closed");
    }
  }
  const c = guarded();
  const pc1 = c.next();
  const pc2 = c.throw(new RangeError("late"));
  const pc3 = c.next();
  log("3 first=" + step(await pc1));
  log("3 afterThrow=" + step(await pc2));
  log("3 third=" + step(await pc3));
  log("3 trace=" + trace.join("|"));

  // 4) a rejected next() does not poison the queue: the generator is done
  trace.length = 0;
  async function* boom() {
    yield "ok";
    throw new TypeError("inside");
  }
  const d = boom();
  log("4 first=" + step(await d.next()));
  log("4 second=" + await d.next().then(
    function (r: any) { return step(r); },
    function (e: any) { return "rejected " + e.constructor.name; }
  ));
  log("4 third=" + step(await d.next()));

  // 5) for-await-of with a break closes the async generator
  trace.length = 0;
  const e = source("e");
  for await (const v of e) {
    if (v === "e2") break;
  }
  log("5 trace=" + trace.join("|"));
  log("5 dead=" + step(await e.next()));

  // 6) an async generator awaits the values it yields
  async function* awaiting() {
    yield Promise.resolve("resolved");
    yield "plain";
  }
  const f = awaiting();
  log("6 first=" + step(await f.next()));
  log("6 second=" + step(await f.next()));
  log("6 done=" + step(await f.next()));

  // 7) the async iterator is its own iterable, and is NOT a sync iterable
  const g = source("g");
  log("7 selfAsync=" + ((g as any)[Symbol.asyncIterator]() === g));
  log("7 noSyncIterator=" + ((g as any)[Symbol.iterator] === undefined));
  await g.return(undefined as any);

  // 8) return() on a never-started async generator skips the body
  trace.length = 0;
  const h = source("h");
  log("8 return=" + step(await h.return("never" as any)));
  log("8 trace=" + JSON.stringify(trace.join("|")));

  console.log("end");
})();
