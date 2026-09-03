// node:stream — the Readable.prototype async-iteration helper family
// (toArray/forEach/map/filter/reduce/some/every/find/drop/take/flatMap),
// isErrored/isReadable/isDisturbed, the six fromWeb/toWeb bridges, and
// .wrap(). No network — everything here is deterministic and in-process.
// Every "Node says" value below is measured with a real Node v20.19.5 via
// `node -e "..."` on this machine, not assumed from the docs.
//
// Every async call below uses a TOP-LEVEL `await` (module scope, not inside
// a nested `async function`), which is the one form this engine resolves
// synchronously against an already-settled promise — the same convention
// `tests/node_fs_promises.test.ts` documents ("the interim event loop...
// a top-level awaited call resolves immediately"). `await` INSIDE a nested
// async function called without awaiting it does NOT get this treatment —
// tried first, and the describe/test block below ran before that function's
// post-`await` continuation ever got a turn, which is a real, separate
// finding: `rts:test`'s runner does not drain the microtask queue between
// the top-level script and the test bodies it invokes.
//
// The `addAbortSignal` + `AbortController#abort()` combination KILLS THE
// WHOLE PROCESS on this engine (confirmed: real Node does not crash there,
// even with zero 'error' listeners — it suppresses the "unhandled error"
// throw specifically for an abort-triggered destroy). That repro is
// isolated in the sibling `-crash` file so it doesn't take this file's
// other assertions down with it; a SAFE addAbortSignal test (with an
// 'error' listener attached, so the crash path is never entered) is here
// instead, to keep that much of the surface measurable.
import { describe, test, expect } from "rts:test";
import stream, { Readable, Writable, Duplex } from "node:stream";

// `expect(x).toEqual(y)` is NOT deep equality in this engine's `rts:test` —
// `crates/rts-std/src/test/mod.rs` registers it as the literal SAME
// function as `toBe` (`("toEqual", equality::to_be)`), i.e. `===`. Two
// array literals with identical contents are never `===`, so
// `expect([3,4,5]).toEqual([3,4,5])` fails in this harness regardless of
// what produced either side — confirmed with a plain literal-vs-literal
// comparison, nothing to do with streams. This is a cross-cutting finding
// about the test framework itself, well outside node:dns/node:stream, so
// every array comparison below uses this small helper instead of the
// broken matcher.
function arrayEq(actual: unknown[], expected: unknown[]): boolean {
    return JSON.stringify(actual) === JSON.stringify(expected);
}

const toArrayResult = await Readable.from([1, 2, 3]).toArray();
const toArrayIsArray = Array.isArray(toArrayResult);

const mappedStream = Readable.from([1, 2, 3]).map((x: number) => x * 2);
const mapIsReadable = mappedStream instanceof Readable;
const mapHasThen = typeof (mappedStream as any).then === "function";
const mapResult = await mappedStream.toArray();

const reduceNoInit = await Readable.from([1, 2, 3]).reduce((a: number, b: number) => a + b);
const reduceWithInit = await Readable.from([1, 2, 3]).reduce((a: number, b: number) => a + b, 10);
const reduceExplicitUndefinedInit = await Readable.from([1, 2, 3]).reduce((a: any, b: any) => a + b, undefined);

let emptyReduceCode = "";
let emptyReduceName = "";
let emptyReduceIsTypeError = false;
try {
    await Readable.from([]).reduce((a: number, b: number) => a + b);
} catch (e: any) {
    emptyReduceCode = e.code;
    emptyReduceName = e.name;
    emptyReduceIsTypeError = e instanceof TypeError;
}

const someSource = Readable.from([1, 2, 3, 4, 5]);
const someResult = await someSource.some((x: number) => x === 3);
const someDestroyed = someSource.destroyed;

const everySource = Readable.from([1, 2, 3, 4, 5]);
const everyResult = await everySource.every((x: number) => x < 3);
const everyDestroyed = everySource.destroyed;

const findSource = Readable.from([1, 2, 3, 4, 5]);
const findResult = await findSource.find((x: number) => x === 3);
const findDestroyed = findSource.destroyed;

const dropResult = await Readable.from([1, 2, 3, 4, 5]).drop(2).toArray();
const takeResult = await Readable.from([1, 2, 3, 4, 5]).take(2).toArray();

const flatMapArrayResult = await Readable.from([1, 2, 3]).flatMap((x: number) => [x, x * 10]).toArray();
const flatMapBufferResult = await Readable.from([1]).flatMap((_x: number) => Buffer.from([65, 66])).toArray();
const flatMapStringResult = await Readable.from([1]).flatMap((_x: number) => "hi").toArray();

const chainedResult = await Readable.from([1, 2, 3, 4])
    .filter((x: number) => x % 2 === 0)
    .map((x: number) => x * 10)
    .toArray();

let forEachSum = 0;
await Readable.from([1, 2, 3]).forEach((x: number) => {
    forEachSum += x;
});

// `.constructor.name` reads "Object" for EVERY stream instance here, not
// only the four derived ones below — even a plain `new Readable()` has it
// (confirmed separately: prototype chain is fine, `instanceof Readable` is
// `true`, `Object.getPrototypeOf(x) === Readable.prototype` is `true` —
// only the `.constructor` back-reference itself is never wired to point at
// the class). So this is a general engine gap, not something particular to
// node:stream; asserted here as `instanceof` (which DOES pass) rather than
// `.constructor.name` (which would fail for reasons this module cannot fix).
const dropIsReadableInstance = Readable.from([1, 2, 3]).drop(1) instanceof Readable;
const takeIsReadableInstance = Readable.from([1, 2, 3]).take(1) instanceof Readable;
const filterIsReadableInstance = Readable.from([1, 2, 3]).filter(() => true) instanceof Readable;
const flatMapIsReadableInstance = Readable.from([1, 2, 3]).flatMap((x: number) => [x]) instanceof Readable;
const forEachCtorName = Readable.from([1, 2, 3]).forEach(() => {}).constructor.name;
const toArrayCtorName = Readable.from([1, 2, 3]).toArray().constructor.name;
const someCtorName = Readable.from([1, 2, 3]).some(() => true).constructor.name;
// One representative RED case for the general `.constructor.name` gap,
// kept separate from the `instanceof` shape check above.
const dropCtorNameIsObjectNotReadable = Readable.from([1, 2, 3]).drop(1).constructor.name;

// ---- isErrored / isReadable / isDisturbed ------------------------------
const sReadable = Readable.from([1, 2, 3]);
const isReadableBeforeConsume = stream.isReadable(sReadable);
sReadable.on("data", () => {});
const isDisturbedAfterConsume = stream.isDisturbed(sReadable);

const sErr = Readable.from([1, 2, 3], { objectMode: true } as any);
sErr.on("error", () => {});
const isErroredBefore = stream.isErrored(sErr);
sErr.destroy(new Error("boom"));
const isErroredAfter = stream.isErrored(sErr);
const erroredMessage = (sErr as any).errored && (sErr as any).errored.message;

const predicateFnsOk = ["isErrored", "isReadable", "isDisturbed", "addAbortSignal"].every((name) => typeof (stream as any)[name] === "function");

// ---- addAbortSignal, SAFE path (an 'error' listener is attached, so the
// crash this engine has on the unlistened path — see the `-crash` file —
// is never reached). Real Node: destroy reason is a DOMException named
// "AbortError". RTS's own message text differs from Node's
// ("This operation was aborted" vs Node's "The operation was aborted") —
// asserting the name, which is what code actually branches on.
const ac = new AbortController();
const sAbort = Readable.from([1, 2, 3]);
stream.addAbortSignal(ac.signal, sAbort);
let abortCaught: any = null;
sAbort.on("error", (e: any) => {
    abortCaught = e;
});
const destroyedBeforeAbort = sAbort.destroyed;
ac.abort();
const destroyedAfterAbort = sAbort.destroyed;
const abortErrorName = abortCaught && abortCaught.name;

// ---- fromWeb / toWeb, all three families -------------------------------
const web = Readable.toWeb(Readable.from([1, 2, 3]));
const toWebCtorName = (web as any).constructor.name;
const reader = (web as any).getReader();
const first = await reader.read();
const toWebFirstChunk = first.value;

const backFromWeb = Readable.fromWeb(web);
const fromWebIsReadable = backFromWeb instanceof Readable;
const fromWebRest = await backFromWeb.toArray();

const writableToWebChunks: unknown[] = [];
const wSink = new Writable({
    objectMode: true,
    write(chunk: unknown, _enc: unknown, cb: () => void) {
        writableToWebChunks.push(chunk);
        cb();
    },
} as any);
const webWritable = Writable.toWeb(wSink) as any;
const writer = webWritable.getWriter();
await writer.write("a");
await writer.write("b");
await writer.close();

const dpair = Duplex.toWeb(
    new Duplex({
        objectMode: true,
        read() {},
        write(_chunk: unknown, _enc: unknown, cb: () => void) {
            cb();
        },
    } as any),
) as any;
const duplexToWebHasBoth = !!dpair.readable && !!dpair.writable;

// ---- .wrap() — the streams-v1 adapter ----------------------------------
// 'data' is synchronous here (this module's own doc); 'end' is delivered
// from a loop source — so a real wait (top-level `await` on a timer
// promise) is what gives that loop turn a chance to run before the
// assertion below reads `wrapEnded`, the same idea
// `tests/node_stream.test.ts`'s own `r6`/`setTimeout` case relies on.
const wrapCollected: number[] = [];
let wrapEnded = false;
{
    const { EventEmitter } = await import("node:events");
    class OldSchool extends EventEmitter {
        pause() {}
        resume() {}
    }
    const old = new OldSchool();
    const wrapped: any = new Readable({ objectMode: true, read() {} } as any).wrap(old as any);
    wrapped.on("data", (d: number) => wrapCollected.push(d));
    wrapped.on("end", () => {
        wrapEnded = true;
    });
    old.emit("data", 1);
    old.emit("data", 2);
    old.emit("end");
    await new Promise((resolve) => setTimeout(resolve, 20));
}

describe("node:stream — async-iteration helpers, predicates, web bridges, wrap", () => {
    test("toArray() -> Promise<Array>", () => {
        expect(toArrayIsArray).toBe(true);
        expect(toArrayResult.length).toBe(3);
        expect(toArrayResult[2]).toBe(3);
    });

    test("map() returns a Readable synchronously, not a Promise", () => {
        expect(mapIsReadable).toBe(true);
        expect(mapHasThen).toBe(false);
    });
    test("map() over toArray() applies fn to every element", () => {
        expect(mapResult[0]).toBe(2);
        expect(mapResult[1]).toBe(4);
        expect(mapResult[2]).toBe(6);
    });

    test("reduce(fn) with no initial value uses the first chunk as the seed", () => expect(reduceNoInit).toBe(6));
    test("reduce(fn, 10) uses the given initial value", () => expect(reduceWithInit).toBe(16));
    test("reduce(fn, undefined) treats explicit undefined as a REAL initial value (NaN), not as omitted", () => {
        expect(Number.isNaN(reduceExplicitUndefinedInit as number)).toBe(true);
    });

    test("reduce() over an empty stream with no initial value rejects TypeError (matches Node)", () => {
        expect(emptyReduceIsTypeError).toBe(true);
        expect(emptyReduceName).toBe("TypeError");
    });
    test("reduce() empty/no-initial error carries code ERR_MISSING_ARGS (RED: RTS's error has no .code at all)", () => {
        expect(emptyReduceCode).toBe("ERR_MISSING_ARGS");
    });

    test("some(fn) short-circuits on the first truthy answer and destroys the source", () => {
        expect(someResult).toBe(true);
        expect(someDestroyed).toBe(true);
    });
    test("every(fn) short-circuits on the first falsy answer and destroys the source", () => {
        expect(everyResult).toBe(false);
        expect(everyDestroyed).toBe(true);
    });
    test("find(fn) short-circuits on the first match and destroys the source", () => {
        expect(findResult).toBe(3);
        expect(findDestroyed).toBe(true);
    });

    test("drop(2) skips the first N chunks", () => expect(arrayEq(dropResult, [3, 4, 5])).toBe(true));
    test("take(2) stops after N chunks", () => expect(arrayEq(takeResult, [1, 2])).toBe(true));

    test("flatMap(fn) over a plain array return spreads its elements", () => expect(arrayEq(flatMapArrayResult, [1, 10, 2, 20, 3, 30])).toBe(true));

    // The module's own doc claims this is a KNOWN divergence from Node
    // ("A Buffer/typed array is NOT special-cased and so IS flattened
    // byte-by-byte, which diverges from Node's 'treat as one chunk' rule").
    // Measured against real Node: that claim is WRONG. Node has no
    // "treat as one chunk" rule for Buffers/typed arrays at all — a
    // returned Buffer is iterated (Buffer extends Uint8Array, whose default
    // iterator yields byte values) exactly like this engine does. So this
    // is NOT a divergence — it is confirmed agreement, asserted here to
    // correct the doc's own claim.
    test("flatMap(fn) over a returned Buffer flattens byte-by-byte (matches Node — the doc's 'diverges' claim is wrong)", () => {
        expect(arrayEq(flatMapBufferResult, [65, 66])).toBe(true);
    });

    // The SAME doc also claims "a string treated as ONE chunk... matching
    // Node, which special-cases strings the same way". Measured: Node does
    // NOT special-case strings in flatMap either — `Readable.from([1]).
    // flatMap(() => "hi")` answers `["h","i"]` in real Node (a string is
    // iterable by code point, and flatMap iterates anything iterable, full
    // stop). This IS a real divergence, in the OPPOSITE direction from what
    // the doc describes: RTS treats a string as one chunk where Node
    // flattens it. Asserting Node's real answer (RED).
    test("flatMap(fn) over a returned string flattens by code point (RED: RTS keeps it as one chunk, backwards from the doc's claim)", () => {
        expect(arrayEq(flatMapStringResult, ["h", "i"])).toBe(true);
    });

    test("chained filter().map().toArray() composes lazily over one pull loop", () => expect(arrayEq(chainedResult, [20, 40])).toBe(true));
    test("forEach(fn) visits every chunk", () => expect(forEachSum).toBe(6));

    test("drop/take/filter/flatMap return a Readable instance (checked via instanceof, see comment above)", () => {
        expect(dropIsReadableInstance).toBe(true);
        expect(takeIsReadableInstance).toBe(true);
        expect(filterIsReadableInstance).toBe(true);
        expect(flatMapIsReadableInstance).toBe(true);
    });
    test("forEach/toArray/some return a Promise (Promise's own .constructor.name is NOT affected by the gap above)", () => {
        expect(forEachCtorName).toBe("Promise");
        expect(toArrayCtorName).toBe("Promise");
        expect(someCtorName).toBe("Promise");
    });
    test(".constructor.name on a stream instance reads 'Object', not 'Readable' (RED: general rts-node class-construction gap, not stream-specific)", () => {
        expect(dropCtorNameIsObjectNotReadable).toBe("Readable");
    });

    test("stream.isReadable/isErrored/isDisturbed/addAbortSignal are all functions", () => expect(predicateFnsOk).toBe(true));
    test("isReadable(stream) is true before any consumption starts", () => expect(isReadableBeforeConsume).toBe(true));
    test("isDisturbed(stream) becomes true once a 'data' listener starts the flow", () => expect(isDisturbedAfterConsume).toBe(true));
    test("isErrored(stream) flips true only after .destroy(err) actually runs", () => {
        expect(isErroredBefore).toBe(false);
        expect(isErroredAfter).toBe(true);
        expect(erroredMessage).toBe("boom");
    });

    test("addAbortSignal(signal, stream) — SAFE path (listener attached): destroys with a real AbortError", () => {
        expect(destroyedBeforeAbort).toBe(false);
        expect(destroyedAfterAbort).toBe(true);
        expect(abortErrorName).toBe("AbortError");
    });

    test("Readable.toWeb()/.getReader() delivers chunks through a real ReadableStream", () => {
        expect(toWebCtorName).toBe("ReadableStream");
        expect(toWebFirstChunk).toBe(1);
    });
    test("Readable.fromWeb() over a partially-drained web stream continues from where the reader left off", () => {
        expect(fromWebIsReadable).toBe(true);
        expect(arrayEq(fromWebRest, [2, 3])).toBe(true);
    });
    test("Writable.toWeb()/.getWriter() relays writes into the Node sink", () => expect(arrayEq(writableToWebChunks, ["a", "b"])).toBe(true));
    test("Duplex.toWeb() exposes both a readable and a writable side", () => expect(duplexToWebHasBoth).toBe(true));

    test(".wrap() forwards 'data' synchronously from the legacy source", () => expect(arrayEq(wrapCollected, [1, 2])).toBe(true));
    test(".wrap() forwards 'end', delivered from a loop source (needs a real wait — see module doc)", () => expect(wrapEnded).toBe(true));
});
