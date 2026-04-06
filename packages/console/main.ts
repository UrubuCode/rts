import { io, process, type WritableStream } from "rts";
import { std } from "process";

type ConsoleArg = string | number | boolean | bigint | symbol | object | null | undefined;

type TableInput = Record<string, unknown> | Array<Record<string, unknown>> | Array<unknown>;

type ConsoleLabel = string | undefined;

interface ConsoleDirOptions {
  depth?: number;
}

function formatArg(arg: ConsoleArg, depth = 2, seen = new Set<object>()): string {
  if (arg === null) {
    return "null";
  }

  if (arg === undefined) {
    return "undefined";
  }

  const type = typeof arg;
  if (type === "string") {
    return arg as string;
  }

  if (type === "number" || type === "boolean" || type === "bigint") {
    return String(arg);
  }

  if (type === "symbol") {
    return (arg as symbol).toString();
  }

  if (type === "function") {
    return `[Function ${(arg as Function).name || "anonymous"}]`;
  }

  const value = arg as object;
  if (seen.has(value)) {
    return "[Circular]";
  }

  if (depth <= 0) {
    return "[Object]";
  }

  seen.add(value);

  if (Array.isArray(arg)) {
    const items = (arg as Array<ConsoleArg>).map((item) => formatArg(item, depth - 1, seen));
    seen.delete(value);
    return `[${items.join(", ")}]`;
  }

  const entries = Object.entries(value as Record<string, unknown>)
    .slice(0, 50)
    .map(([key, val]) => `${key}: ${formatArg(val as ConsoleArg, depth - 1, seen)}`);
  seen.delete(value);
  return `{ ${entries.join(", ")} }`;
}

function formatWithTokens(
  first: ConsoleArg,
  rest: Array<ConsoleArg>,
  depth: number,
): string {
  if (typeof first !== "string") {
    return [formatArg(first, depth), ...rest.map((item) => formatArg(item, depth))].join(" ");
  }

  let cursor = 0;
  const text = first.replace(/%[sdifoOj%]/g, (token) => {
    if (token === "%%") {
      return "%";
    }

    const value = rest[cursor++];
    if (token === "%s") {
      return String(value);
    }
    if (token === "%d" || token === "%i") {
      return value === undefined ? "NaN" : String(Number(value));
    }
    if (token === "%f") {
      return value === undefined ? "NaN" : String(Number(value));
    }
    if (token === "%j") {
      try {
        return JSON.stringify(value);
      } catch {
        return "[Circular]";
      }
    }
    if (token === "%o" || token === "%O") {
      return formatArg(value, depth);
    }
    return token;
  });

  const remaining = rest.slice(cursor).map((item) => formatArg(item, depth));
  if (remaining.length === 0) {
    return text;
  }

  return `${text} ${remaining.join(" ")}`;
}

function normalizedLabel(label?: string): string {
  const value = label?.trim();
  return value && value.length > 0 ? value : "default";
}

function renderTable(input: TableInput): string {
  const rows: Array<Record<string, unknown>> = Array.isArray(input)
    ? input.map((value, index) =>
        typeof value === "object" && value !== null
          ? (value as Record<string, unknown>)
          : { value, index },
      )
    : [input];

  if (rows.length === 0) {
    return "(empty)";
  }

  const columns = new Set<string>();
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      columns.add(key);
    }
  }

  const orderedColumns = ["(index)", ...Array.from(columns)];
  const data: Array<Array<string>> = rows.map((row, index) => {
    const line = [String(index)];
    for (const key of orderedColumns.slice(1)) {
      line.push(formatArg(row[key] as ConsoleArg, 1));
    }
    return line;
  });

  const widths = orderedColumns.map((column, index) => {
    let width = column.length;
    for (const row of data) {
      width = Math.max(width, row[index].length);
    }
    return width;
  });

  const makeLine = (cells: Array<string>): string =>
    `| ${cells.map((cell, idx) => cell.padEnd(widths[idx], " ")).join(" | ")} |`;

  const separator = `|-${widths.map((width) => "-".repeat(width)).join("-|-")}-|`;
  const lines = [makeLine(orderedColumns), separator, ...data.map((row) => makeLine(row))];
  return lines.join("\n");
}

export function log(message: string): void {
  io.stdout_write(message);
}

export function info(message: string): void {
  io.stdout_write(message);
}

export function debug(message: string): void {
  io.stdout_write(message);
}

export function warn(message: string): void {
  io.stderr_write(message);
}

export function error(message: string): void {
  io.stderr_write(message);
}

export class Console {
  public stdout: WritableStream;
  public stderr: WritableStream;

  public readonly _stdout: WritableStream;
  public readonly _stderr: WritableStream;
  public readonly Console: typeof Console;

  private readonly counters: Map<string, number>;
  private readonly timers: Map<string, number>;
  private groupDepth: number;
  private readonly indentUnit: string;
  private inspectDepth: number;

  constructor(stdout: WritableStream = std.out, stderr: WritableStream = std.err) {
    this.stdout = stdout;
    this.stderr = stderr;
    this._stdout = stdout;
    this._stderr = stderr;
    this.Console = Console;
    this.counters = new Map();
    this.timers = new Map();
    this.groupDepth = 0;
    this.indentUnit = "  ";
    this.inspectDepth = 2;
  }

  public log(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, data);
  }

  public info(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, data);
  }

  public debug(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, data);
  }

  public warn(...data: Array<ConsoleArg>): void {
    this.emit(this.stderr, data);
  }

  public error(...data: Array<ConsoleArg>): void {
    this.emit(this.stderr, data);
  }

  public clear(): void {
    this.stdout.write("\u001b[2J\u001b[3J\u001b[H");
  }

  public dir(item: ConsoleArg, options?: ConsoleDirOptions): void {
    const depth = options?.depth ?? this.inspectDepth;
    this.emit(this.stdout, [formatArg(item, depth)]);
  }

  public dirxml(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, data);
  }

  public table(tabularData: TableInput): void {
    this.emit(this.stdout, [renderTable(tabularData)]);
  }

  public trace(message?: ConsoleArg, ...optionalParams: Array<ConsoleArg>): void {
    const formatted =
      message === undefined
        ? "Trace"
        : formatWithTokens(message, optionalParams, this.inspectDepth);
    const stack = new Error(formatted).stack || formatted;
    this.emit(this.stderr, [stack]);
  }

  public assert(value: unknown, ...message: Array<ConsoleArg>): void {
    if (value) {
      return;
    }

    if (message.length === 0) {
      this.emit(this.stderr, ["Assertion failed"]);
      return;
    }

    this.emit(this.stderr, ["Assertion failed:", ...message]);
  }

  public count(label?: ConsoleLabel): void {
    const key = normalizedLabel(label);
    const next = (this.counters.get(key) ?? 0) + 1;
    this.counters.set(key, next);
    this.emit(this.stdout, [`${key}: ${next}`]);
  }

  public countReset(label?: ConsoleLabel): void {
    const key = normalizedLabel(label);
    if (!this.counters.has(key)) {
      this.emit(this.stderr, [`Count for '${key}' does not exist`]);
      return;
    }

    this.counters.set(key, 0);
  }

  public profile(label?: ConsoleLabel): void {
    this.emit(this.stdout, [`profile '${normalizedLabel(label)}' is not implemented yet`]);
  }

  public profileEnd(label?: ConsoleLabel): void {
    this.emit(this.stdout, [`profileEnd '${normalizedLabel(label)}' is not implemented yet`]);
  }

  public time(label?: ConsoleLabel): void {
    this.timers.set(normalizedLabel(label), Number(process.clock_now()));
  }

  public timeLog(label?: ConsoleLabel, ...data: Array<ConsoleArg>): void {
    const key = normalizedLabel(label);
    const start = this.timers.get(key);
    if (start === undefined) {
      this.emit(this.stderr, [`Timer '${key}' does not exist`]);
      return;
    }

    const elapsed = Number(process.clock_now()) - start;
    this.emit(this.stdout, [`${key}: ${elapsed.toFixed(3)}ms`, ...data]);
  }

  public timeEnd(label?: ConsoleLabel): void {
    const key = normalizedLabel(label);
    const start = this.timers.get(key);
    if (start === undefined) {
      this.emit(this.stderr, [`Timer '${key}' does not exist`]);
      return;
    }

    const elapsed = Number(process.clock_now()) - start;
    this.timers.delete(key);
    this.emit(this.stdout, [`${key}: ${elapsed.toFixed(3)}ms`]);
  }

  public timeStamp(label?: ConsoleLabel): void {
    const key = normalizedLabel(label);
    this.emit(
      this.stdout,
      [`[timestamp] ${key} @ ${Number(process.clock_now()).toFixed(3)}ms`],
    );
  }

  public takeHeapSnapshot(label?: ConsoleLabel): string {
    const id = `heap-${normalizedLabel(label)}-${Math.floor(Number(process.clock_now()))}`;
    this.emit(this.stdout, [`takeHeapSnapshot -> ${id}`]);
    return id;
  }

  public group(...label: Array<ConsoleArg>): void {
    if (label.length > 0) {
      this.emit(this.stdout, label);
    }
    this.groupDepth += 1;
  }

  public groupCollapsed(...label: Array<ConsoleArg>): void {
    this.group(...label);
  }

  public groupEnd(): void {
    this.groupDepth = Math.max(0, this.groupDepth - 1);
  }

  public record(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, ["record", ...data]);
  }

  public recordEnd(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, ["recordEnd", ...data]);
  }

  public screenshot(...data: Array<ConsoleArg>): void {
    this.emit(this.stdout, ["screenshot", ...data]);
  }

  public write(...data: Array<ConsoleArg>): void {
    const line = data.length === 0 ? "" : formatWithTokens(data[0], data.slice(1), this.inspectDepth);
    this.stdout.write(`${line}`);
  }

  private emit(stream: WritableStream, data: Array<ConsoleArg>): void {
    const indent = this.indentUnit.repeat(this.groupDepth);
    const line =
      data.length === 0
        ? ""
        : formatWithTokens(data[0], data.slice(1), this.inspectDepth);
    stream.write(`${indent}${line}\n`);
  }
}

export const console = new Console(std.out, std.err);
