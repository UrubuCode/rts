import { io, process, globals } from "rts";

const COUNTER_PREFIX = "console:count:";
const TIMER_PREFIX = "console:time:";

function key(prefix: string, label: string): string {
  return `${prefix}${label}`;
}

function formatValue(value: any): string {
  if (value === undefined) {
    return "undefined";
  }
  if (value === null) {
    return "null";
  }
  return `${value}`;
}

function formatArgs(args: any[]): string {
  let out = "";
  for (let i = 0; i < args.length; i += 1) {
    if (i > 0) {
      out += " ";
    }
    out += formatValue(args[i]);
  }
  return out;
}

function writeLineStdout(args: any[]): void {
  io.stdout_write(`${formatArgs(args)}\n`);
}

function writeLineStderr(args: any[]): void {
  io.stderr_write(`${formatArgs(args)}\n`);
}

function readNumber(raw: any): number {
  const parsed = Number(raw);
  if (parsed !== parsed) {
    return 0;
  }
  return parsed;
}

function timerElapsed(label: string): number | undefined {
  const stored = globals.get(key(TIMER_PREFIX, label));
  if (stored === undefined) {
    return undefined;
  }
  return process.clock_now() - readNumber(stored);
}

export function log(...args: any[]): void {
  writeLineStdout(args);
}

export function info(...args: any[]): void {
  writeLineStdout(args);
}

export function debug(...args: any[]): void {
  writeLineStdout(args);
}

export function warn(...args: any[]): void {
  writeLineStderr(args);
}

export function error(...args: any[]): void {
  writeLineStderr(args);
}

export function trace(...args: any[]): void {
  const message = formatArgs(args);
  if (message.length === 0) {
    io.stderr_write("Trace\n");
    return;
  }
  io.stderr_write(`Trace: ${message}\n`);
}

export function assert(condition: any, ...args: any[]): void {
  if (condition) {
    return;
  }

  if (args.length === 0) {
    io.stderr_write("Assertion failed\n");
    return;
  }

  io.stderr_write(`Assertion failed: ${formatArgs(args)}\n`);
}

export function count(label = "default"): void {
  const storageKey = key(COUNTER_PREFIX, label);
  const current = readNumber(globals.get(storageKey));
  const next = current + 1;
  globals.set(storageKey, `${next}`);
  io.stdout_write(`${label}: ${next}\n`);
}

export function countReset(label = "default"): void {
  const storageKey = key(COUNTER_PREFIX, label);
  if (!globals.has(storageKey)) {
    io.stderr_write(`Count for '${label}' does not exist\n`);
    return;
  }
  globals.remove(storageKey);
}

export function time(label = "default"): void {
  globals.set(key(TIMER_PREFIX, label), `${process.clock_now()}`);
}

export function timeLog(label = "default", ...args: any[]): void {
  const elapsed = timerElapsed(label);
  if (elapsed === undefined) {
    io.stderr_write(`No such label '${label}' for console.timeLog()\n`);
    return;
  }

  if (args.length === 0) {
    io.stdout_write(`${label}: ${elapsed}ms\n`);
    return;
  }

  io.stdout_write(`${label}: ${elapsed}ms ${formatArgs(args)}\n`);
}

export function timeEnd(label = "default"): void {
  const elapsed = timerElapsed(label);
  if (elapsed === undefined) {
    io.stderr_write(`No such label '${label}' for console.timeEnd()\n`);
    return;
  }

  globals.remove(key(TIMER_PREFIX, label));
  io.stdout_write(`${label}: ${elapsed}ms\n`);
}

export function clear(): void {
  io.stdout_write("\u001bc");
}

export const console = {
  assert,
  clear,
  count,
  countReset,
  debug,
  error,
  info,
  log,
  time,
  timeEnd,
  timeLog,
  trace,
  warn,
};

globalThis.console = console;
global.console = console;
