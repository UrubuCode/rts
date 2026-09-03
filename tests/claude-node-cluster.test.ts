// node:cluster — process spawning wearing cluster's name; NO IPC channel
// exists (crates/rts-node/src/cluster.rs's own `//!` doc). `cluster.fork()`
// genuinely re-spawns THIS SAME program as a new OS process (with
// NODE_UNIQUE_ID set so the child's own isPrimary reads false) — so this
// fixture guards every fork()/describe()/test() call behind
// `cluster.isPrimary`: the forked child re-runs this whole file from the top,
// and without the guard it would try to fork AGAIN (it does not, since the
// guard makes isPrimary false there) and would also re-register every test
// as its own separate process report. With the guard, a worker process does
// nothing but the plain synchronous namespace checks below and exits
// immediately — no recursion, no second report, no hang.
//
// The other finding this fixture pins by execution, and it is sharper than
// "events are merely deferred": `cluster.on(...)`/`cluster.emit(...)` are
// COMPLETELY DEAD, always, no matter how many times fork() is called
// afterward. Root cause, isolated below: `cluster.rs`'s `namespace()` builds
// the module object and chains the `EventEmitter` PROTOTYPE onto it
// (`set_prototype_in`, so `.on`/`.emit`/`.listenerCount` all resolve as
// functions), but — unlike every other EventEmitter-shaped object this
// crate builds (`dgram`'s sockets, `http2`'s sessions, `inspector`'s
// `Session`, all of which call `put_member(..., "__events__", listeners)`
// right after construction) — never gives the namespace object its OWN
// `__events__` storage object. `events::add_listener` reads `__events__` off
// `this`, finds `undefined`, and every following read/write against it is a
// no-op against `undefined` rather than a store — so a listener attached
// with `.on()` is silently dropped and `.listenerCount()` reads 0 right
// after registering one. Proven directly: patching `(cluster as any).
// __events__ = {}` before calling `.on()` makes the exact same listener
// fire correctly (see the last describe block below).

import { describe, test, expect } from "rts:test";
import * as cluster from "node:cluster";
import { time } from "rts";

// --- synchronous namespace shape (safe in primary AND worker) --------------
const isPrimaryType = typeof cluster.isPrimary;
const isPrimaryEqualsIsMaster = cluster.isPrimary === cluster.isMaster;
const isWorkerIsOpposite = cluster.isWorker === !cluster.isPrimary;
const schedRR = cluster.SCHED_RR;
const schedNone = cluster.SCHED_NONE;
const schedConstantsOk = schedRR === 2 && schedNone === 1;
// Windows has no libuv IOCP round-robin story, so the default here is
// SCHED_NONE on Windows (matches this module's own doc and Node's real
// per-platform default).
const defaultPolicyOk = cluster.schedulingPolicy === (process.platform === "win32" ? schedNone : schedRR);
const settingsInitiallyObject = typeof cluster.settings === "object";
const workersInitiallyEmpty = Object.keys(cluster.workers).length === 0;

cluster.setupPrimary({ exec: "does-not-matter.js" });
const settingsAfterSetup = (cluster.settings as any).exec === "does-not-matter.js";
cluster.setupMaster({ exec: "alias-form.js" }); // setupMaster is an alias
const settingsAfterAlias = (cluster.settings as any).exec === "alias-form.js";

if (cluster.isPrimary) {
    // --- fork() genuinely spawns a real OS process ----------------------------
    const forkEvents: string[] = [];
    const onlineEvents: string[] = [];
    const exitEvents: { id: number; code: any; signal: any }[] = [];
    cluster.on("fork", (w: any) => forkEvents.push(String(w.id)));
    cluster.on("online", (w: any) => onlineEvents.push(String(w.id)));
    cluster.on("exit", (w: any, code: any, signal: any) => exitEvents.push({ id: w.id, code, signal }));

    const workerA = cluster.fork();
    const workerAId = workerA.id;
    const pidOk = typeof workerA.process.pid === "number" && workerA.process.pid > 0;
    const workersHasA = typeof (cluster.workers as any)[workerAId] === "object";

    // --- there is no IPC: named by absence, not silently missing ---------------
    const noSendOk = typeof (workerA as any).send === "undefined";
    const noIsConnectedOk = typeof (workerA as any).isConnected === "undefined";
    const noIsDeadOk = typeof (workerA as any).isDead === "undefined";
    const exitedAfterDisconnectUndefinedOk = (workerA as any).exitedAfterDisconnect === undefined;

    // listenerCount right after registering is the first symptom: it reads 0,
    // not 1 — the listener array was never actually stored anywhere.
    const listenerCountAfterOnOk = (cluster as any).listenerCount("fork") === 1;

    // kill it, then give the background waiter thread (polls every 20ms) time
    // to observe the exit and queue it, then run TWO more fork()s (each one's
    // own pump() call is the only thing that ever flushes A's queue) — a
    // generous, repeated attempt to give delivery every chance before
    // concluding it never happens.
    workerA.kill();
    time.sleep_ms(150);
    const workerB = cluster.fork();
    time.sleep_ms(150);
    workerB.kill();
    const workerC = cluster.fork();
    time.sleep_ms(150);
    workerC.kill();
    time.sleep_ms(150);

    // Real Node fires 'fork' synchronously (before fork() even returns) and
    // 'online' shortly after; this module's own doc promises both, plus
    // 'exit' once the child dies. Asserting Node's real, documented shape —
    // expected to stay RED, root-caused above and pinned below.
    const gotForkOk = forkEvents.indexOf(String(workerAId)) >= 0;
    const gotOnlineOk = onlineEvents.indexOf(String(workerAId)) >= 0;
    const gotExitOk = exitEvents.some((e) => e.id === workerAId);

    const workersNoLongerHasA = (cluster.workers as any)[workerAId] === undefined;

    // --- root cause, pinned directly ---------------------------------------
    // Read in `fork()`'s own code: the per-worker instance it builds has the
    // exact same omission as the namespace — `make_instance` over a
    // prototype chained onto "EventEmitter", but no `put_member(...,
    // "__events__", ...)` call anywhere for it either. So this is not a
    // one-off on the module object: it is the same missing initialization
    // step, twice, everywhere this module builds something meant to emit.
    const workerEventsMissingTooOk = (workerA as any).__events__ === undefined;
    const namespaceEventsMissingOk = (cluster as any).__events__ === undefined;
    // Patching the namespace's store in by hand, on the exact same `cluster`
    // object used above, makes the exact same kind of listener fire — this
    // is what confirms the missing store IS the whole cause, not a symptom
    // of something else.
    (cluster as any).__events__ = {};
    let patchedListenerRan = false;
    cluster.on("__probe__" as any, () => {
        patchedListenerRan = true;
    });
    (cluster as any).emit("__probe__");
    const patchProvesRootCauseOk = patchedListenerRan === true;

    describe("node:cluster — namespace shape (primary)", () => {
        test("isPrimary is a boolean, true in this process", () => {
            expect(isPrimaryType).toBe("boolean");
            expect(cluster.isPrimary).toBe(true);
        });
        test("isMaster is an alias for isPrimary", () => expect(isPrimaryEqualsIsMaster).toBe(true));
        test("isWorker is the opposite of isPrimary", () => expect(isWorkerIsOpposite).toBe(true));
        test("SCHED_RR=2 / SCHED_NONE=1", () => expect(schedConstantsOk).toBe(true));
        test("schedulingPolicy defaults per-platform", () => expect(defaultPolicyOk).toBe(true));
        test("settings/workers start as an empty object", () => {
            expect(settingsInitiallyObject).toBe(true);
            expect(workersInitiallyEmpty).toBe(true);
        });
        test("setupPrimary()/setupMaster() store the settings object verbatim", () => {
            expect(settingsAfterSetup).toBe(true);
            expect(settingsAfterAlias).toBe(true);
        });
    });

    describe("node:cluster — fork() is a real OS process", () => {
        test("worker.process.pid is a real positive pid", () => expect(pidOk).toBe(true));
        test("cluster.workers gains an entry keyed by the worker's id", () => expect(workersHasA).toBe(true));
        test("there is no IPC: worker.send/.isConnected/.isDead are undefined, not stubs", () => {
            expect(noSendOk).toBe(true);
            expect(noIsConnectedOk).toBe(true);
            expect(noIsDeadOk).toBe(true);
        });
        test("exitedAfterDisconnect stays undefined (no graceful .disconnect() exists)", () =>
            expect(exitedAfterDisconnectUndefinedOk).toBe(true));
    });

    describe("node:cluster — events, per real Node's documented shape (expected RED here)", () => {
        test("listenerCount('fork') reads 1 right after cluster.on('fork', ...)", () =>
            expect(listenerCountAfterOnOk).toBe(true));
        test("'fork' reaches the listener", () => expect(gotForkOk).toBe(true));
        test("'online' reaches the listener", () => expect(gotOnlineOk).toBe(true));
        test("'exit' reaches the listener once the child is reaped", () => expect(gotExitOk).toBe(true));
        test("a reaped worker is removed from cluster.workers (this DOES work — rebuild_workers runs from pump, independent of emit)", () =>
            expect(workersNoLongerHasA).toBe(true));
    });

    describe("node:cluster — root cause of the dead events, pinned by execution", () => {
        test("the cluster namespace object never got an __events__ store", () =>
            expect(namespaceEventsMissingOk).toBe(true));
        test("neither did the per-worker instance fork() built — the same omission, twice", () =>
            expect(workerEventsMissingTooOk).toBe(true));
        test("...and patching one in by hand makes the SAME cluster.on()/.emit() work", () =>
            expect(patchProvesRootCauseOk).toBe(true));
    });
} else {
    // A forked worker: prove it took the OTHER branch and stop. No fork(),
    // no describe()/test() — this file is being re-run as the CHILD process
    // cluster.fork() spawned, and this is where that recursion is cut off.
}
