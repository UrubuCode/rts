// node:diagnostics_channel — in-process publish/subscribe bus (ambient `.ts`
// prelude, NOT primordial). Pure JS/TS over primordial Map/Array/Function —
// Node's own module is the same. Subscriber identity for unsubscribe() is real
// value identity (the handler lives in a `.ts` array), which the object-backed
// Rust version could not guarantee across the callback-arg reification boundary.
//
// The named-channel registry is a module-level Map (Node uses WeakRef so an
// unsubscribed channel can be GC'd; RTS keeps a strong ref — a documented,
// behaviour-preserving deviation: a re-`channel(name)` returns the same object).
//
// bindStore/runStores enter a bound store's context only when the store exposes
// a `.run(value, cb)` (node:async_hooks AsyncLocalStorage); without that module
// the sync publish + fn call still run correctly, the cross-async-boundary store
// propagation is simply absent (honest — never a wrong value).

const __dcRegistry: Map<string, any> = new Map();

// A store bound to a channel via bindStore (real class, not an object literal:
// literals with fn-valued props hoist to a colliding __fnprop).
class __DcStoreBinding {
  store: any = null;
  transform: any = null;
}

class Channel {
  name: any = "";
  __subs: any[] = [];
  __stores: any[] = [];
  constructor(name: any = "") {
    this.name = name;
    this.__subs = [];
    this.__stores = [];
  }

  get hasSubscribers(): any { return this.__subs.length > 0; }

  publish(message: any): void {
    const subs = this.__subs.slice(0);
    for (let i = 0; i < subs.length; i++) {
      const fn = subs[i];
      fn(message, this.name);
    }
  }

  subscribe(onMessage: any): void {
    if (typeof onMessage !== "function") {
      throw __dcErr("ERR_INVALID_ARG_TYPE", "The \"onMessage\" argument must be a function");
    }
    this.__subs.push(onMessage);
  }

  unsubscribe(onMessage: any): any { return __dcRemove(this, onMessage); }

  bindStore(store: any, transform: any = undefined): void {
    const b = new __DcStoreBinding();
    b.store = store;
    b.transform = transform === undefined ? null : transform;
    this.__stores.push(b);
  }

  unbindStore(store: any): any { return __dcUnbind(this, store); }

  runStores(context: any, fn: any, thisArg: any = undefined, a1: any = undefined, a2: any = undefined, a3: any = undefined): any {
    this.publish(context);
    return __dcRunStores(this, context, fn, thisArg, a1, a2, a3);
  }
}

// ---- free helpers (boolean-heavy / identity logic) ------------------------
function __dcErr(code: any, msg: any): any {
  const e: any = new Error(msg);
  e.code = code;
  return e;
}

function __dcRemove(ch: any, onMessage: any): any {
  const subs = ch.__subs;
  const out: any[] = [];
  let found = false;
  for (let i = 0; i < subs.length; i++) {
    if (found) { out.push(subs[i]); }
    else if (subs[i] === onMessage) { found = true; }
    else { out.push(subs[i]); }
  }
  ch.__subs = out;
  return found;
}

function __dcUnbind(ch: any, store: any): any {
  const st = ch.__stores;
  const out: any[] = [];
  let found = false;
  for (let i = 0; i < st.length; i++) {
    if (st[i].store === store) { found = true; }
    else { out.push(st[i]); }
  }
  ch.__stores = out;
  return found;
}

// Enter every bound store (best-effort: only stores exposing `.run`) then call
// fn. Nesting is unrolled to a fixed depth via recursion over the binding list.
function __dcRunStores(ch: any, context: any, fn: any, thisArg: any, a1: any, a2: any, a3: any): any {
  return __dcEnter(ch.__stores, 0, context, fn, thisArg, a1, a2, a3);
}
function __dcEnter(stores: any, idx: any, context: any, fn: any, thisArg: any, a1: any, a2: any, a3: any): any {
  if (idx >= stores.length) { return fn.call(thisArg, a1, a2, a3); }
  const b = stores[idx];
  const value = b.transform === null ? context : b.transform(context);
  const store = b.store;
  if (store !== null && store !== undefined && typeof store.run === "function") {
    return store.run(value, () => { return __dcEnter(stores, idx + 1, context, fn, thisArg, a1, a2, a3); });
  }
  return __dcEnter(stores, idx + 1, context, fn, thisArg, a1, a2, a3);
}

// ---- module functions -----------------------------------------------------
function __dcChannel(name: any): any {
  const key = "" + name;
  const existing = __dcRegistry.get(key);
  if (existing !== undefined) { return existing; }
  const ch = new Channel(name);
  __dcRegistry.set(key, ch);
  return ch;
}

function __dcHasSubscribers(name: any): any {
  const ch = __dcRegistry.get("" + name);
  if (ch === undefined) { return false; }
  return ch.hasSubscribers;
}

function __dcSubscribe(name: any, onMessage: any): void { __dcChannel(name).subscribe(onMessage); }
function __dcUnsubscribe(name: any, onMessage: any): any { return __dcChannel(name).unsubscribe(onMessage); }

function __dcTracingChannel(nameOrChannels: any): any { return new TracingChannel(nameOrChannels); }

// ==== TracingChannel =======================================================
class TracingChannel {
  start: any = null;
  end: any = null;
  asyncStart: any = null;
  asyncEnd: any = null;
  error: any = null;
  constructor(nameOrChannels: any = "") {
    if (typeof nameOrChannels === "string") {
      const n = nameOrChannels;
      this.start = __dcChannel("tracing:" + n + ":start");
      this.end = __dcChannel("tracing:" + n + ":end");
      this.asyncStart = __dcChannel("tracing:" + n + ":asyncStart");
      this.asyncEnd = __dcChannel("tracing:" + n + ":asyncEnd");
      this.error = __dcChannel("tracing:" + n + ":error");
    } else if (nameOrChannels !== null && nameOrChannels !== undefined) {
      this.start = nameOrChannels.start;
      this.end = nameOrChannels.end;
      this.asyncStart = nameOrChannels.asyncStart;
      this.asyncEnd = nameOrChannels.asyncEnd;
      this.error = nameOrChannels.error;
    }
  }

  get hasSubscribers(): any {
    return this.start.hasSubscribers || this.end.hasSubscribers || this.asyncStart.hasSubscribers ||
      this.asyncEnd.hasSubscribers || this.error.hasSubscribers;
  }

  subscribe(subscribers: any): void {
    if (subscribers === null || subscribers === undefined) { return; }
    if (typeof subscribers.start === "function") { this.start.subscribe(subscribers.start); }
    if (typeof subscribers.end === "function") { this.end.subscribe(subscribers.end); }
    if (typeof subscribers.asyncStart === "function") { this.asyncStart.subscribe(subscribers.asyncStart); }
    if (typeof subscribers.asyncEnd === "function") { this.asyncEnd.subscribe(subscribers.asyncEnd); }
    if (typeof subscribers.error === "function") { this.error.subscribe(subscribers.error); }
  }

  unsubscribe(subscribers: any): any { return __tcUnsub(this, subscribers); }

  traceSync(fn: any, context: any = undefined, thisArg: any = undefined, a1: any = undefined, a2: any = undefined, a3: any = undefined): any {
    return __tcTraceSync(this, fn, context, thisArg, a1, a2, a3);
  }
  tracePromise(fn: any, context: any = undefined, thisArg: any = undefined, a1: any = undefined, a2: any = undefined, a3: any = undefined): any {
    return __tcTracePromise(this, fn, context, thisArg, a1, a2, a3);
  }
  traceCallback(fn: any, position: any = undefined, context: any = undefined, thisArg: any = undefined, a1: any = undefined, a2: any = undefined, a3: any = undefined): any {
    return __tcTraceCallback(this, fn, position, context, thisArg, a1, a2, a3);
  }
}

function __tcUnsub(tc: any, subscribers: any): any {
  if (subscribers === null || subscribers === undefined) { return false; }
  let ok = true;
  if (typeof subscribers.start === "function") { if (!tc.start.unsubscribe(subscribers.start)) { ok = false; } }
  if (typeof subscribers.end === "function") { if (!tc.end.unsubscribe(subscribers.end)) { ok = false; } }
  if (typeof subscribers.asyncStart === "function") { if (!tc.asyncStart.unsubscribe(subscribers.asyncStart)) { ok = false; } }
  if (typeof subscribers.asyncEnd === "function") { if (!tc.asyncEnd.unsubscribe(subscribers.asyncEnd)) { ok = false; } }
  if (typeof subscribers.error === "function") { if (!tc.error.unsubscribe(subscribers.error)) { ok = false; } }
  return ok;
}

function __tcCtx(context: any): any { return context === undefined ? {} : context; }

function __tcTraceSync(tc: any, fn: any, context: any, thisArg: any, a1: any, a2: any, a3: any): any {
  const ctx = __tcCtx(context);
  tc.start.publish(ctx);
  try {
    const result = fn.call(thisArg, a1, a2, a3);
    ctx.result = result;
    tc.end.publish(ctx);
    return result;
  } catch (err) {
    ctx.error = err;
    tc.error.publish(ctx);
    tc.end.publish(ctx);
    throw err;
  }
}

function __tcTracePromise(tc: any, fn: any, context: any, thisArg: any, a1: any, a2: any, a3: any): any {
  const ctx = __tcCtx(context);
  tc.start.publish(ctx);
  let promise: any;
  try {
    promise = fn.call(thisArg, a1, a2, a3);
  } catch (err) {
    ctx.error = err;
    tc.error.publish(ctx);
    tc.end.publish(ctx);
    throw err;
  }
  tc.end.publish(ctx);
  if (promise === null || promise === undefined || typeof promise.then !== "function") {
    return promise;
  }
  tc.asyncStart.publish(ctx);
  const settled = promise.then(
    (value: any) => { ctx.result = value; tc.asyncEnd.publish(ctx); return value; },
    (err: any) => { ctx.error = err; tc.error.publish(ctx); tc.asyncEnd.publish(ctx); throw err; },
  );
  return settled;
}

function __tcTraceCallback(tc: any, fn: any, position: any, context: any, thisArg: any, a1: any, a2: any, a3: any): any {
  const ctx = __tcCtx(context);
  const args: any[] = [];
  if (a1 !== undefined) { args.push(a1); }
  if (a2 !== undefined) { args.push(a2); }
  if (a3 !== undefined) { args.push(a3); }
  let pos = typeof position === "number" ? position : args.length - 1;
  if (pos < 0) { pos = 0; }
  const original = args[pos];
  tc.asyncStart.publish(ctx);
  const wrapped = (err: any, res: any) => {
    if (err !== null && err !== undefined) { ctx.error = err; tc.error.publish(ctx); }
    ctx.result = res;
    tc.asyncEnd.publish(ctx);
    if (typeof original === "function") { return original(err, res); }
    return undefined;
  };
  args[pos] = wrapped;
  tc.start.publish(ctx);
  try {
    const result = fn.call(thisArg, args[0], args[1], args[2]);
    tc.end.publish(ctx);
    return result;
  } catch (e) {
    ctx.error = e;
    tc.error.publish(ctx);
    tc.end.publish(ctx);
    throw e;
  }
}
