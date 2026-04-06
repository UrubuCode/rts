import { io, process } from "rts";

export interface StdReader {
  read(maxBytes?: number): string;
}

export interface StdWriter {
  write(message: string): void;
}

export interface StdHandles {
  in: StdReader;
  out: StdWriter;
  err: StdWriter;
}

export const std: StdHandles = {
  in: {
    read(maxBytes?: number): string {
      if (maxBytes === undefined) {
        return String(io.stdin_read());
      }
      return String(io.stdin_read(maxBytes));
    },
  },
  out: {
    write(message: string): void {
      io.stdout_write(message);
    },
  },
  err: {
    write(message: string): void {
      io.stderr_write(message);
    },
  },
};

export function argv(): Array<string> {
  const raw = process.args();
  if (Array.isArray(raw)) {
    return raw as Array<string>;
  }
  if (typeof raw === "string") {
    return raw.length === 0 ? [] : raw.split(",");
  }
  return [];
}

export function pwd(): string {
  return String(process.cwd());
}

export function getEnv(name: string): string | undefined {
  const value = process.env_get(name);
  if (value === undefined || value === null) {
    return undefined;
  }
  return String(value);
}

export function setEnv(name: string, value: string): void {
  process.env_set(name, value);
}

export function getPid(): number {
  return Number(process.pid());
}

export function getPlatform(): string {
  return String(process.platform());
}

export function getArch(): string {
  return String(process.arch());
}

export function readStdin(maxBytes?: number): string {
  return std.in.read(maxBytes);
}

export function writeStdout(message: string): void {
  std.out.write(message);
}

export function writeStderr(message: string): void {
  std.err.write(message);
}

export function terminate(code = 0): never {
  process.exit(code);
}

export function delay(ms: number): void {
  process.sleep(ms);
}
