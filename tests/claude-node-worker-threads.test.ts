// node:worker_threads — a real OS thread running a real, disconnected engine
// instance. `crates/rts-node/src/worker_threads/mod.rs`'s own doc states the
// delivery model up front: a worker's thread never calls a listener directly,
// it queues native data, and the PARENT turns that into an event on ITS OWN
// thread the next time something pumps the loop. `time.sleep_ms(n)` (from
// `import { time } from "rts"`, documented in `machine/time.rs`) is this
// suite's own established way to give a background thread real wall-clock
// time AND pump the loop while waiting — the same idiom
// `node_net_server.test.ts` already uses for its accept thread. Every
// assertion below was checked once directly against this engine (`rts run`
// on a two-line script) before being written here, since there is no real
// Node's `node:worker_threads` output to compare shapes against beyond what
// its own documentation states (the mechanics — real OS threads, no shared
// heap — are this engine's own, not Node's, by construction).
//
// A whole separate finding lives in `claude-node-worker-threads-crash.test.ts`:
// an uncaught exception INSIDE a worker's thread does not become an 'error'
// event on that Worker (as this module's own doc promises) — it takes down
// the ENTIRE PROCESS, parent thread included. `require` inside an `eval:
// true` worker source is one ordinary-looking way to trigger it (see that
// file), so every worker source below uses `import`, never `require`.
import { describe, test, expect } from "rts:test";
import {
    Worker,
    isMainThread,
    isInternalThread,
    threadId,
    threadName,
    workerData,
    parentPort,
    resourceLimits,
    getEnvironmentData,
    setEnvironmentData,
    isTerminating,
} from "node:worker_threads";
import * as wt from "node:worker_threads";
import { time } from "rts";

// ── main-thread properties ───────────────────────────────────────────────────
const mainIsMainThread = isMainThread;
const mainIsInternalThread = isInternalThread;
const mainThreadId = threadId;
const mainThreadName = threadName;
const mainParentPort = parentPort;
const mainResourceLimitsIsEmptyObject =
    typeof resourceLimits === "object" && resourceLimits !== null && Object.keys(resourceLimits).length === 0;
const mainIsTerminating = isTerminating();

// ── DIVERGENCE (not documented anywhere in the module doc): Node's own
// `worker_threads.workerData` on the MAIN thread is `null` (checked directly:
// `node -e "console.log(require('worker_threads').workerData)"` on the real
// Node v20 installed on this machine answers `null`). This engine answers
// `undefined` — `registry::side`'s `None` arm defaults to `Portable::Undefined`
// rather than a null value.
const mainWorkerDataIsNode = workerData === null;
const mainWorkerDataActual = workerData;

// ── MessageChannel/MessagePort exist as GLOBALS (rts-std) but are NOT
// re-exported from the `node:worker_threads` module namespace, where real
// Node exports both (checked directly: `typeof require('worker_threads')
// .MessageChannel === 'function'` on real Node). The module's own doc line
// ("the local pair below is real and same-thread only") reads as though the
// pair lives HERE; it lives in `rts-std/src/globals/events/channel.rs`
// instead, reached only as a bare global.
const messageChannelOnNamespace = typeof (wt as any).MessageChannel;
const messagePortOnNamespace = typeof (wt as any).MessagePort;
const messageChannelAsGlobal = typeof (globalThis as any).MessageChannel;

// ── the local MessageChannel pair itself DOES work, same-thread, via the
// global — both `onmessage` and `addEventListener('message', …)` ───────────
// Two SEPARATE channels, so registering the second listener cannot also
// catch the first channel's already-queued message once the loop pumps.
const channelA: any = new (globalThis as any).MessageChannel();
let viaOnmessage: any = "not-yet";
channelA.port2.onmessage = (event: any) => {
    viaOnmessage = event.data;
};
channelA.port1.postMessage({ ping: 1, tag: "a" });

const channelB: any = new (globalThis as any).MessageChannel();
let viaListener: any = "not-yet";
channelB.port2.addEventListener("message", (event: any) => {
    viaListener = event.data;
});
channelB.port1.postMessage("second-message");
time.sleep_ms(0);

// ── getEnvironmentData / setEnvironmentData — plain values round-trip ───────
const envBeforeSet = getEnvironmentData("claude-env-key");
setEnvironmentData("claude-env-key", "a-string-value");
const envAfterSet = getEnvironmentData("claude-env-key");
setEnvironmentData("claude-env-num", 123);
const envNum = getEnvironmentData("claude-env-num");

// ── BUG: an ARRAY loses every element crossing `portable()`'s walk. Root
// cause verified directly: `portable::portable`'s array branch reads each
// element with `entry::get_member(context, value, &index.to_string())` — a
// STRING key built at runtime — which does not resolve an array's element
// (the exact same shape of bug `claude-node-vm.test.ts` documents for
// `vm.compileFunction`'s `param_names`, and for the same reason:
// `entry::get_indexed(value, entry::make_number(i))`, which every OTHER
// array reader in this crate — e.g. `wasi/mod.rs`'s `read_string_array` —
// uses instead, is what actually reaches an array's storage). The array
// comes back the RIGHT LENGTH with every element `undefined`.
setEnvironmentData("claude-env-array", [10, 20, 30]);
const envArray: any = getEnvironmentData("claude-env-array");
const envArrayLengthOk = Array.isArray(envArray) && envArray.length === 3;
const envArrayValuesOk = envArrayLengthOk && envArray[0] === 10 && envArray[1] === 20 && envArray[2] === 30;

// ── eval: false is refused BY NAME, as an 'error' event, not a throw ─────────
const badWorker = new Worker("some/nonexistent/file.js");
let badWorkerErrorFired = false;
let badWorkerErrorMessage = "";
let badWorkerExitFired = false;
let badWorkerExitCode = -1;
badWorker.on("error", (e: any) => {
    badWorkerErrorFired = true;
    badWorkerErrorMessage = e && e.message;
});
badWorker.on("exit", (code: any) => {
    badWorkerExitFired = true;
    badWorkerExitCode = code;
});
time.sleep_ms(100);

// ── eval: true, a worker that runs to completion and exits cleanly ──────────
const okWorker = new Worker("1 + 1;", { eval: true });
const threadIdRightAfterConstruct = okWorker.threadId;
let okExitFired = false;
let okExitCode = -1;
okWorker.on("exit", (code: any) => {
    okExitFired = true;
    okExitCode = code;
});
time.sleep_ms(200);

// ── workerData crosses INTO a worker, and a non-array shape survives ────────
const wdSrc = `
    import { workerData, parentPort } from "node:worker_threads";
    parentPort.postMessage({ seenWorkerData: workerData });
`;
const wdWorker = new Worker(wdSrc, { eval: true, workerData: { name: "claude", n: 7, flag: true } });
let wdReceived: any = null;
wdWorker.on("message", (m: any) => {
    wdReceived = m;
});
time.sleep_ms(300);

// ── postMessage both directions (object, no array — the shape that works) ───
const echoSrc = `
    import { parentPort, receiveMessageOnPort } from "node:worker_threads";
    import { time } from "rts";
    let msg = undefined;
    for (let i = 0; i < 60; i++) {
        const got = receiveMessageOnPort(parentPort);
        if (got !== undefined) { msg = got.message; break; }
        time.sleep_ms(10);
    }
    parentPort.postMessage({ echoed: msg, workerSawIt: msg !== undefined });
`;
const echoWorker = new Worker(echoSrc, { eval: true });
let echoReceived: any = null;
echoWorker.on("message", (m: any) => {
    echoReceived = m;
});
echoWorker.postMessage({ hello: "parent", count: 3 });
time.sleep_ms(400);

// ── worker.terminate() + the cooperative isTerminating() poll ───────────────
const pollSrc = `
    import { isTerminating } from "node:worker_threads";
    import { time } from "rts";
    let n = 0;
    while (!isTerminating() && n < 400) {
        n++;
        time.sleep_ms(5);
    }
`;
const pollWorker = new Worker(pollSrc, { eval: true });
let pollExited = false;
let pollExitCode = -1;
pollWorker.on("exit", (code: any) => {
    pollExited = true;
    pollExitCode = code;
});
time.sleep_ms(30);
pollWorker.terminate();
time.sleep_ms(300);

describe("node:worker_threads — main-thread properties", () => {
    test("isMainThread is true", () => expect(mainIsMainThread).toBe(true));
    test("isInternalThread is always false (documented)", () => expect(mainIsInternalThread).toBe(false));
    test("threadId is 0 on the main thread", () => expect(mainThreadId).toBe(0));
    test("threadName is null when unset", () => expect(mainThreadName).toBe(null));
    test("parentPort is null on the main thread", () => expect(mainParentPort).toBe(null));
    test("resourceLimits is an empty object (documented)", () => expect(mainResourceLimitsIsEmptyObject).toBe(true));
    test("isTerminating() is false on the main thread", () => expect(mainIsTerminating).toBe(false));
});

describe("node:worker_threads — workerData default (RED: undocumented divergence)", () => {
    test("Node answers null for unset workerData on main; this engine answers undefined", () =>
        expect(mainWorkerDataIsNode).toBe(true));
});

describe("node:worker_threads — MessageChannel/MessagePort export (RED: undocumented gap)", () => {
    test("Node exports MessageChannel from node:worker_threads", () => expect(messageChannelOnNamespace).toBe("function"));
    test("Node exports MessagePort from node:worker_threads", () => expect(messagePortOnNamespace).toBe("function"));
    test("...but it IS a real global here", () => expect(messageChannelAsGlobal).toBe("function"));
});

describe("node:worker_threads — the local MessageChannel pair (via the global)", () => {
    test("onmessage delivers the posted value", () => expect(viaOnmessage && (viaOnmessage as any).ping).toBe(1));
    test("addEventListener('message', …) also delivers", () => expect(viaListener).toBe("second-message"));
});

describe("node:worker_threads — getEnvironmentData/setEnvironmentData", () => {
    test("unset key answers undefined", () => expect(envBeforeSet).toBe(undefined));
    test("a string value round-trips", () => expect(envAfterSet).toBe("a-string-value"));
    test("a number value round-trips", () => expect(envNum).toBe(123));
});

describe("node:worker_threads — getEnvironmentData/setEnvironmentData, an ARRAY (RED: real bug)", () => {
    test("the array keeps its length", () => expect(envArrayLengthOk).toBe(true));
    test("the array keeps its VALUES — currently every element is lost", () => expect(envArrayValuesOk).toBe(true));
});

describe("node:worker_threads — eval: false is refused by name", () => {
    test("'error' fires, not a throw at construction", () => expect(badWorkerErrorFired).toBe(true));
    test("the error names the { eval: true } requirement", () =>
        expect(badWorkerErrorMessage.indexOf("eval: true") !== -1).toBe(true));
    test("'exit' still fires after the refusal, with a nonzero code", () => expect(badWorkerExitFired).toBe(true));
    test("...specifically exit code 1", () => expect(badWorkerExitCode).toBe(1));
});

describe("node:worker_threads — eval: true, runs and exits cleanly", () => {
    test("threadId is assigned synchronously, before the thread finishes", () =>
        expect(typeof threadIdRightAfterConstruct).toBe("number"));
    test("'exit' fires", () => expect(okExitFired).toBe(true));
    test("...with exit code 0", () => expect(okExitCode).toBe(0));
});

describe("node:worker_threads — workerData crosses into a worker (non-array shape)", () => {
    test("the worker sees the object's string field", () => expect(wdReceived && wdReceived.seenWorkerData && wdReceived.seenWorkerData.name).toBe("claude"));
    test("the worker sees the object's number field", () => expect(wdReceived && wdReceived.seenWorkerData && wdReceived.seenWorkerData.n).toBe(7));
    test("the worker sees the object's boolean field", () => expect(wdReceived && wdReceived.seenWorkerData && wdReceived.seenWorkerData.flag).toBe(true));
});

describe("node:worker_threads — postMessage, parent -> worker -> parent (poll model)", () => {
    test("the worker actually saw the posted message", () => expect(echoReceived && echoReceived.workerSawIt).toBe(true));
    test("the echoed object's field round-trips", () => expect(echoReceived && echoReceived.echoed && echoReceived.echoed.hello).toBe("parent"));
    test("the echoed object's number round-trips", () => expect(echoReceived && echoReceived.echoed && echoReceived.echoed.count).toBe(3));
});

describe("node:worker_threads — terminate() + isTerminating() (cooperative, documented)", () => {
    test("the worker exits after terminate() is called", () => expect(pollExited).toBe(true));
    test("...with exit code 0 (a cooperative stop, not a kill)", () => expect(pollExitCode).toBe(0));
});
