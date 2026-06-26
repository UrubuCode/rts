// Faithful TypeScript `console` — the global console object for the new engine,
// written in `.ts` instead of hardcoded in codegen. The front-end NAMES nothing
// here (no `is_console_ident`, no `lower_console_log`); `console` is an ordinary
// ambient `Console` instance and `console.log(...)` an ordinary method call.
//
// ## The irreducible logic stays runtime-side (one source of truth)
// Two operations the `.ts` cannot express are exposed via the PRIVATE `engine.*`:
//   * `engine.display(x)` — render ONE value to its display string (ToString for
//     scalars/strings, `[ … ]` for arrays, `{ … }` for objects); the
//     object-vs-scalar decision is made at RUNTIME, not statically in the front.
//   * `engine.print_line(s)` / `engine.eprint_line(s)` — write one formatted line
//     to stdout / stderr (capture-aware, the same sink the tests read).
// The variadic join with single spaces is plain `.ts` below.

class Console {
  // Join `args` into one display line: each element via `engine.display`,
  // separated by single spaces. Empty → "" (an empty line).
  __format(args: any[]): string {
    let out = "";
    for (let i = 0; i < args.length; i++) {
      if (i > 0) out = out + " ";
      out = out + engine.display(args[i]);
    }
    return out;
  }
  // stdout sinks
  log(...args: any[]): void {
    engine.print_line(this.__format(args));
  }
  info(...args: any[]): void {
    engine.print_line(this.__format(args));
  }
  debug(...args: any[]): void {
    engine.print_line(this.__format(args));
  }
  dir(arg: any): void {
    engine.print_line(engine.display(arg));
  }
  // stderr sinks
  error(...args: any[]): void {
    engine.eprint_line(this.__format(args));
  }
  warn(...args: any[]): void {
    engine.eprint_line(this.__format(args));
  }
  // Best-effort stubs for the rest of the console surface (#310/#311). Output does
  // NOT match Node/Bun byte-for-byte (no real table rendering, indent tracking, or
  // timers) — the goal is that the common calls COMPILE and run without bailing.
  // `group`/`groupCollapsed`/`table`/`trace` still write a line (Node-ish); the
  // visual/stateful ones (`groupEnd`/`time*`/`count*`/`clear`/`assert`) are no-ops.
  table(data?: any): void {
    engine.print_line(engine.display(data));
  }
  group(...args: any[]): void {
    engine.print_line(this.__format(args));
  }
  groupCollapsed(...args: any[]): void {
    engine.print_line(this.__format(args));
  }
  groupEnd(): void {}
  trace(...args: any[]): void {
    engine.eprint_line(this.__format(args));
  }
  count(label?: any): void {}
  countReset(label?: any): void {}
  time(label?: any): void {}
  timeEnd(label?: any): void {}
  timeLog(...args: any[]): void {}
  clear(): void {}
  assert(cond?: any, ...args: any[]): void {}
}

const console = new Console();
