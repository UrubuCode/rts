// Web event-model — MessageChannel/MessagePort — rts-shared stdlib prelude
// (NOT primordial; no native syntax). `Event`/`EventTarget`/`AbortSignal`/
// `AbortController` used to live here too; DRAIN_MOTOR §11 (owner
// 2026-07-24 correction) reimplemented them as `#[rtse::class]` at FULL
// PARITY (options `{once}`, spec dispatch order, `once` removed BEFORE its
// callback, `!defaultPrevented`) — see `rts-std/src/globals/event_target/`
// + `rts-std/src/globals/abort/`. MessagePort delivers via queueMicrotask.

class MessagePort {
  onmessage: any = null;
  // Wired by MessageChannel (double-underscore internal, same rationale as
  // AbortSignal.__doAbort).
  __peer: any = null;
  postMessage(data: any): void {
    const peer = this.__peer;
    queueMicrotask(() => {
      const h = peer.onmessage;
      if (h !== null && h !== undefined) { h({ data: data }); }
    });
  }
  start(): void {}
  close(): void {}
}

class MessageChannel {
  port1: MessagePort;
  port2: MessagePort;
  constructor() {
    this.port1 = new MessagePort();
    this.port2 = new MessagePort();
    this.port1.__peer = this.port2;
    this.port2.__peer = this.port1;
  }
}
